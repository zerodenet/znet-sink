#!/usr/bin/env python3
"""Exercise disposable Zero v0.0.2/v0.0.3 runtimes without touching the client.

Requires both real executables. No TUN, system proxy, provider credentials, or
installed state is used. JSON captures contain only this synthetic runtime.
"""
import argparse
import base64
import copy
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time


def call(endpoint, frame):
    frame = {**frame, "id": "compatibility-check"}
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as stream:
        stream.settimeout(10)
        stream.connect(str(endpoint))
        stream.sendall(json.dumps(frame).encode() + b"\n")
        response = json.loads(stream.makefile("rb").readline())
    assert response.get("id") == frame["id"], {"request": frame, "response": response}
    return response


def query(endpoint, name):
    params = {"filter": {}} if name == "active_flows" else {}
    response = call(endpoint, {"type": "query", "request": {name: params}})
    assert response["ok"], response
    return response["result"][name]


def command(endpoint, method, config):
    return call(endpoint, {"type": "command", "method": method, "params": {"config": config}})


def check(binary, series, captures):
    output = subprocess.check_output([str(binary), "--version"], text=True, timeout=30)
    version = next(line.split(":", 1)[1].strip() for line in output.splitlines() if line.startswith("build_id:"))
    assert version.startswith(series + "-") or version == series, version
    with tempfile.TemporaryDirectory(prefix="znet-kernel-compat-", dir="/tmp") as temporary:
        root = Path(temporary)
        endpoint = root / "control.sock"
        config = {"schema_version": 1, "inbounds": [],
                  "outbounds": [{"tag": "direct", "protocol": {"type": "direct"}}],
                  "mode": {"type": "rule"}, "route": {"rules": [], "final": {"type": "direct"}}}
        path = root / "config.json"
        path.write_text(json.dumps(config))
        environment = {**os.environ, "ZERO_DNS_STATE_DIR": str(root / "dns"), "NO_COLOR": "1"}
        subprocess.run([str(binary), "validate", str(path)], env=environment,
                       check=True, capture_output=True, timeout=30)
        with (root / "runtime.log").open("wb") as log:
            process = subprocess.Popen([str(binary), "run", "--control-socket", str(endpoint),
                                        "--parent-lifetime-stdin", str(path)],
                                       env=environment, stdin=subprocess.PIPE, stdout=log, stderr=log)
            try:
                deadline = time.monotonic() + 20
                while not endpoint.exists():
                    if process.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError("Disposable kernel did not become ready")
                    time.sleep(0.05)
                snapshots = {name: query(endpoint, name) for name in
                             ["health", "capabilities", "runtime", "stats", "policies", "active_flows", "tun_status"]}
                assert snapshots["health"]["engine_build_id"] == version
                assert not snapshots["tun_status"]["running"]
                identity = snapshots["runtime"]["core_instance_id"]
                revision = snapshots["runtime"]["config_revision"]
                for feature in ["capabilities", "control_api", "config_schema", "error_codes"]:
                    contract = snapshots["capabilities"]["contracts"][feature]
                    assert contract["minimum_supported"] <= 1 <= contract["current"], contract
                result = command(endpoint, "config.apply_runtime", config)
                assert result["ok"] and result["result"]["accepted"], result
                applied = result["result"]["result"]
                assert applied["persistence"] == "runtime_only", applied
                assert applied["core_instance_id"] == identity
                assert applied["config_revision"] > revision
                assert json.loads(path.read_text()) == config
                before = query(endpoint, "runtime")
                future = {**config, "schema_version": 999}
                assert not command(endpoint, "config.apply_runtime", future)["ok"]
                after = query(endpoint, "runtime")
                assert after["core_instance_id"] == before["core_instance_id"]
                assert after["config_revision"] == before["config_revision"]
                assert json.loads(path.read_text()) == config
                new_route = copy.deepcopy(config)
                new_route["route"].update({"auto_outbounds": [], "final_mode": "flow"})
                route_result = command(endpoint, "config.validate", new_route)
                assert route_result["ok"] == (series == "0.0.3"), route_result
                # Synthetic keys only. Validation does not start this device or
                # contact a peer; installation and authorization are unrelated.
                wireguard = copy.deepcopy(new_route)
                wireguard["outbounds"].append({"tag": "wg", "protocol": {
                    "type": "wireguard", "private_key": base64.b64encode(bytes([1]) * 32).decode(),
                    "addresses": ["10.10.0.11/32"], "mtu": 1420, "peers": [{
                        "public_key": base64.b64encode(bytes([2]) * 32).decode(),
                        "endpoint": "127.0.0.1:51820", "allowed_ips": ["192.0.2.0/24"],
                        "keepalive_secs": 25}]}})
                wireguard["route"]["auto_outbounds"] = ["wg"]
                wg_result = command(endpoint, "config.validate", wireguard)
                has_wireguard = any(protocol["protocol"] == "wireguard" and protocol["compiled"]
                                    for protocol in snapshots["capabilities"]["protocols"])
                assert wg_result["ok"] == has_wireguard, wg_result
                captures.mkdir(parents=True, exist_ok=True)
                (captures / f"{series}.json").write_text(json.dumps({"version": version, **snapshots}, indent=2))
                print(f"PASS {version}: queries, runtime apply, rejected-schema rollback, route/WireGuard compatibility", flush=True)
            finally:
                process.stdin.close()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel-v002", required=True, type=Path)
    parser.add_argument("--kernel-v003", required=True, type=Path)
    parser.add_argument("--captures", required=True, type=Path)
    arguments = parser.parse_args()
    check(arguments.kernel_v002.resolve(), "0.0.2", arguments.captures)
    check(arguments.kernel_v003.resolve(), "0.0.3", arguments.captures)
