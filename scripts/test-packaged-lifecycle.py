#!/usr/bin/env python3
"""macOS package smoke test; uses private data and never enables network capture."""
import argparse
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time


def query(endpoint, variant):
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(1)
        stream.connect(str(endpoint))
        stream.sendall((json.dumps({"type": "query", "request": {variant: {}}}) + "\n").encode())
        reply = json.loads(stream.makefile("rb").readline())
        assert reply["ok"], reply
        return reply["result"][variant]


def alive(pid):
    try:
        os.kill(pid, 0)
        # An exited orphan may briefly remain a zombie while launchd reaps it.
        result = subprocess.run(["ps", "-p", str(pid), "-o", "stat="], capture_output=True, text=True)
        return result.returncode == 0 and not result.stdout.strip().startswith("Z")
    except ProcessLookupError:
        return False


def cycle(app, root, label):
    log = root / f"{label}.stderr"
    child_pid = None
    with log.open("wb") as output:
        gui = subprocess.Popen([str(app)], cwd=root, env={**os.environ, "ZNET_SINK_DATA_DIR": str(root)},
                               stdin=subprocess.DEVNULL, stdout=output, stderr=output)
        endpoint = root / "core" / f"zero-control-{gui.pid}.sock"
        try:
            deadline = time.monotonic() + 25
            last_error = None
            while time.monotonic() < deadline:
                assert gui.poll() is None, f"candidate exited early; see {log}"
                try:
                    assert query(endpoint, "health")["healthy"]
                    runtime = query(endpoint, "runtime")
                    observed_pid = runtime["pid"]
                    parent = subprocess.check_output(["ps", "-p", str(observed_pid), "-o", "ppid="], text=True)
                    assert int(parent.strip()) == gui.pid
                    child_pid = observed_pid
                    time.sleep(0.5)
                    assert query(endpoint, "runtime")["pid"] == child_pid
                    break
                except (OSError, ValueError, AssertionError) as error:
                    last_error = error
                    time.sleep(0.1)
            else:
                for diagnostic in [log, root / "logs/gui.log.jsonl", root / "logs/core.log.jsonl"]:
                    if diagnostic.is_file():
                        print(diagnostic.read_text(errors="replace")[-12000:], flush=True)
                raise AssertionError(f"candidate not ready: {last_error}; see {log}")
            gui.kill()  # Deliberately bypass graceful cleanup to test inherited lifetime EOF.
            gui.wait(timeout=5)
            deadline = time.monotonic() + 8
            while alive(child_pid) and time.monotonic() < deadline:
                time.sleep(0.1)
            assert not alive(child_pid), "owned kernel survived forced GUI exit"
            assert not (root / "system-proxy-guard.json").exists(), "capture marker unexpectedly created"
            print(json.dumps({"cycle": label, "guiPid": gui.pid, "kernelPid": child_pid,
                              "ready": True, "kernelExitedAfterGuiKill": True}), flush=True)
        finally:
            if gui.poll() is None:
                gui.kill()
                gui.wait(timeout=5)
            # Only this private endpoint's proven child may be cleaned up.
            if child_pid and alive(child_pid):
                os.kill(child_pid, 9)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app-binary", type=Path, required=True)
    parser.add_argument("--kernel-binary", type=Path, required=True)
    args = parser.parse_args()
    app = args.app_binary.resolve(strict=True)
    kernel = args.kernel_binary.resolve(strict=True)
    before = subprocess.check_output(["scutil", "--proxy"])
    with tempfile.TemporaryDirectory(prefix="znet-smoke-", dir="/tmp") as directory:
        root = Path(directory)
        (root / "core").mkdir()
        shutil.copy2(kernel, root / "core/zero")
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            port = probe.getsockname()[1]
        config = {"core": {"autoStart": True, "autoConnect": False,
                           "executablePath": str(root / "core/zero")},
                  "localProxy": {"host": "127.0.0.1", "port": port},
                  "tun": {"enabled": False}, "ui": {"trafficBallEnabled": False}}
        primary = root / "app-config.json"
        primary.write_text(json.dumps(config))
        cycle(app, root, "first-start")
        assert (root / "app-config.json.bak").is_file()
        primary.write_text('{"core":')
        cycle(app, root, "backup-recovery")
        assert json.loads(primary.read_text())["core"]["autoConnect"] is False
        assert any(root.glob("app-config-corrupt-*.json"))
        print("PASS: package launch, private IPC, backup recovery, forced-exit child cleanup", flush=True)
    assert subprocess.check_output(["scutil", "--proxy"]) == before, "system proxy changed during smoke test"
    print("PASS: system proxy unchanged", flush=True)


if __name__ == "__main__":
    main()
