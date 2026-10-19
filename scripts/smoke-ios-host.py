"""Install/launch the actual no-identity Simulator App and collect bounded evidence."""
import argparse
import json
import os
import pathlib
import re
import shutil
import subprocess
import time
import uuid
from importlib.machinery import SourceFileLoader

verify = SourceFileLoader('ios_host_verifier', str(pathlib.Path(__file__).with_name('verify-ios-host.py'))).load_module()


def run(*args, **kwargs):
    kwargs.setdefault('timeout', 180)
    return subprocess.check_output(args, text=True, **kwargs).strip()


def select_runtime(runtimes, devices):
    candidates = []
    for runtime in runtimes:
        if (not runtime.get('isAvailable') or 'SimRuntime.iOS-' not in runtime['identifier']
                or not re.fullmatch(r'\d+(?:\.\d+)*', runtime['version'])):
            continue
        phones = [device for device in devices.get(runtime['identifier'], [])
                  if device.get('isAvailable') and device['name'].startswith('iPhone')]
        if phones:
            candidates.append((runtime, phones))
    assert candidates, 'no installed available iPhone Simulator runtime'
    return max(candidates, key=lambda pair: tuple(int(part) for part in pair[0]['version'].split('.')))


def main(evidence):
    root = pathlib.Path(evidence)
    package = json.loads((root / 'package-receipt.json').read_text())
    assert package['sdk'] == 'iphonesimulator' and package['noSigningCredentials']
    assert package['signingIdentity'] == 'none' and package['signature'] in ('none', 'ad-hoc')
    app = pathlib.Path(package['app'])
    # This driver is copied only into the CI smoke product, never the device app.
    shutil.copyfile(pathlib.Path(__file__).parent.parent / 'src-tauri/apple/ios-host-smoke.js', app / 'ios-host-smoke.js')
    devices = json.loads(run('xcrun', 'simctl', 'list', 'devices', 'available', '--json'))['devices']
    runtimes = json.loads(run('xcrun', 'simctl', 'list', 'runtimes', '--json'))['runtimes']
    # Choose a reported installed runtime, not dictionary order. iOS 18.5 has
    # a documented WebKit Swift overlay launch defect for older app floors.
    runtime, candidates = select_runtime(runtimes, devices)
    device = candidates[0]['udid']
    print(json.dumps(dict(phase='boot', runtime=runtime['identifier'], version=runtime['version'], device=device)), flush=True)
    (root / 'simulator-environment.json').write_text(json.dumps(dict(runtime=runtime, device=candidates[0]), indent=2) + '\n')
    if candidates[0]['state'] != 'Booted':
        run('xcrun', 'simctl', 'boot', device)
    run('xcrun', 'simctl', 'bootstatus', device, '-b', timeout=300)
    bundle = package['applicationId']
    run('xcrun', 'simctl', 'install', device, str(app))
    nonce = uuid.uuid4().hex
    env = dict(os.environ, SIMCTL_CHILD_ZNET_IOS_UI_SMOKE_NONCE=nonce)
    print(json.dumps(dict(phase='launch', runtime=runtime['identifier'], version=runtime['version'], device=device)), flush=True)
    container = pathlib.Path(run('xcrun', 'simctl', 'get_app_container', device, bundle, 'data'))
    report = container / 'Library/Caches/znet-ios-ui-smoke.json'
    console = root / 'simulator-console.txt'
    with console.open('w') as output:
        process = subprocess.Popen(['xcrun', 'simctl', 'launch', '--console', '--terminate-running-process',
                                    device, bundle], env=env, stdout=output, stderr=subprocess.STDOUT)
        try:
            for _ in range(150):
                assert console.stat().st_size <= 1024 * 1024, 'fresh smoke console exceeded diagnostic budget'
                if report.is_file():
                    value = json.loads(report.read_text())
                    if value.get('nonce') == nonce:
                        (root / 'simulator-smoke.json').write_text(json.dumps(value, indent=2) + '\n')
                        run('xcrun', 'simctl', 'io', device, 'screenshot', str(root / 'simulator.png'))
                        print(json.dumps(value, sort_keys=True), flush=True)
                        print(console.read_text(errors='replace')[-8192:], flush=True)
                        verify.validate_smoke(value, nonce)
                        (root / 'simulator.json').write_text(json.dumps(dict(udid=device, name=candidates[0]['name'],
                            runtime=runtime['identifier'], runtimeVersion=runtime['version'], simulator=True,
                            deviceAcceptance=False, check='actual-WKWebView-and-Rust-IPC'), indent=2) + '\n')
                        return
                if process.poll() is not None:
                    break
                time.sleep(1)
            run('xcrun', 'simctl', 'io', device, 'screenshot', str(root / 'simulator-timeout.png'))
            # This is a fresh test container with no imported profiles/secrets.
            print(console.read_text(errors='replace')[-8192:], flush=True)
            raise RuntimeError('actual Simulator UI/IPC smoke receipt not received; see bounded native console')
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--evidence', required=True)
    main(parser.parse_args().evidence)
