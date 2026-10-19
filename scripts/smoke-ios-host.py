"""Install/launch the actual no-identity Simulator App and collect bounded evidence."""
import argparse
import json
import os
import pathlib
import shutil
import subprocess
import time
import uuid
from importlib.machinery import SourceFileLoader

verify = SourceFileLoader('ios_host_verifier', str(pathlib.Path(__file__).with_name('verify-ios-host.py'))).load_module()


def run(*args, **kwargs):
    return subprocess.check_output(args, text=True, **kwargs).strip()


def main(evidence):
    root = pathlib.Path(evidence)
    package = json.loads((root / 'package-receipt.json').read_text())
    assert package['sdk'] == 'iphonesimulator' and package['noSigningCredentials']
    assert package['signingIdentity'] == 'none' and package['signature'] in ('none', 'ad-hoc')
    app = pathlib.Path(package['app'])
    # This driver is copied only into the CI smoke product, never the device app.
    shutil.copyfile(pathlib.Path(__file__).parent.parent / 'src-tauri/apple/ios-host-smoke.js', app / 'ios-host-smoke.js')
    devices = json.loads(run('xcrun', 'simctl', 'list', 'devices', 'available', '--json'))['devices']
    candidates = [device for runtime, group in devices.items() if 'iOS' in runtime for device in group
                  if device.get('isAvailable') and device['name'].startswith('iPhone')]
    assert candidates, 'no installed iPhone Simulator runtime'
    device = candidates[0]['udid']
    if candidates[0]['state'] != 'Booted':
        run('xcrun', 'simctl', 'boot', device)
    run('xcrun', 'simctl', 'bootstatus', device, '-b')
    bundle = package['applicationId']
    run('xcrun', 'simctl', 'install', device, str(app))
    nonce = uuid.uuid4().hex
    env = dict(os.environ, SIMCTL_CHILD_ZNET_IOS_UI_SMOKE_NONCE=nonce)
    launch = run('xcrun', 'simctl', 'launch', '--terminate-running-process', device, bundle, env=env)
    (root / 'simulator-launch.txt').write_text(launch + '\n')
    container = pathlib.Path(run('xcrun', 'simctl', 'get_app_container', device, bundle, 'data'))
    report = container / 'Library/Caches/znet-ios-ui-smoke.json'
    for _ in range(150):
        if report.is_file():
            value = json.loads(report.read_text())
            if value.get('nonce') == nonce:
                (root / 'simulator-smoke.json').write_text(json.dumps(value, indent=2) + '\n')
                run('xcrun', 'simctl', 'io', device, 'screenshot', str(root / 'simulator.png'))
                verify.validate_smoke(value, nonce)
                print(json.dumps(value, sort_keys=True))
                (root / 'simulator.json').write_text(json.dumps(dict(udid=device, name=candidates[0]['name'],
                    simulator=True, deviceAcceptance=False, check='actual-WKWebView-and-Rust-IPC'), indent=2) + '\n')
                return
        time.sleep(1)
    run('xcrun', 'simctl', 'io', device, 'screenshot', str(root / 'simulator-timeout.png'))
    raise RuntimeError('actual Simulator UI/IPC smoke receipt not received; no simulated success')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--evidence', required=True)
    main(parser.parse_args().evidence)
