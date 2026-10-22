"""Verify an actual main App, distinguishing SDK/architecture from execution."""
import argparse
import hashlib
import json
import pathlib
import plistlib
import re
import shutil
import subprocess
from xml.parsers.expat import ExpatError
from ios_keychain_diagnostics import validate_keychain_diagnostics


def validate_smoke(receipt, nonce):
    assert receipt['schemaVersion'] == 1 and receipt['nonce'] == nonce, 'stale smoke receipt'
    assert receipt['status'] == 'completed', 'actual WebView smoke failed'
    assert receipt['bundleIdentifier'] == 'org.zerodenet.znetsink'
    assert receipt['nativePlugin'] == 'znet-native-io' and receipt['webViewInspectable'] is True
    checks = receipt['checks']
    for key in ['rustIpc', 'nativePresentation', 'fourTabs', 'onboarding', 'aboutAndReturn',
                'vpnUnsupported', 'nativeLicenseAvailable', 'nativePluginVm', 'nativeSecureStorageContract']:
        assert checks.get(key) is True, f'missing actual UI/IPC check: {key}'
    assert checks['applicationId'] == 'org.zerodenet.znetsink'
    assert checks['nativeSecureStorageState'] in ['available', 'unavailable']
    assert checks['nativeSecureStorageAccepted'] is (checks['nativeSecureStorageState'] == 'available'), 'unsupported storage cannot be accepted'
    validate_keychain_diagnostics(checks['nativeKeychainDiagnostics'])
    assert 0 < checks['width'] <= 1024 and checks['height'] > 0
    return receipt


def validate_plugin_lifecycle(receipts, nonce):
    assert len(receipts) == 3, 'three actual native lifecycle observations required'
    generations = []
    for receipt, phase in zip(receipts, ['ready', 'suspended', 'ready']):
        assert receipt['schemaVersion'] == 1 and receipt['nonce'] == nonce
        assert receipt['pid'] > 0 and receipt['pid'] == receipts[0]['pid'], 'app restarted during lifecycle check'
        platform = receipt['platform']
        assert platform['state'] == phase and platform['target']['os'] == 'ios'
        assert receipt['executorAccepting'] is (phase == 'ready'), 'native owner did not seal/reopen admission'
        generations.append(int(platform['generation']))
    assert generations[0] < generations[1] < generations[2], 'stale native lifecycle observation'
    return receipts


def public_plugin_lifecycle(receipts, nonce):
    """Bounded fixed facts only; future native fields never enter console logs."""
    verified = validate_plugin_lifecycle(receipts, nonce)
    return [dict(pid=row['pid'], phase=row['platform']['state'],
                 generation=str(int(row['platform']['generation'])),
                 executorAccepting=row['executorAccepting']) for row in verified]


def public_startup_progress(value, nonce):
    """Progress cannot pass as a final smoke receipt or leak arbitrary native fields."""
    assert value['schemaVersion'] == 1 and value['nonce'] == nonce
    assert value['bundleIdentifier'] == 'org.zerodenet.znetsink'
    assert type(value['pid']) is int and 0 < value['pid'] <= 2**31-1
    assert value['stage'] in ['plugin-initialized', 'webview-attached', 'waiting-for-document',
                              'evaluating-driver', 'javascript-returned']
    result = dict(stage=value['stage'], pid=value['pid'])
    if 'documentScheme' in value:
        assert value['documentScheme'] in ['tauri', 'http', 'https', 'about', 'absent-or-other']
        assert type(value['documentIsLoading']) is bool
        result.update(documentScheme=value['documentScheme'], documentIsLoading=value['documentIsLoading'])
    return result


def validate_package(info, sdk, arches, symbols, build_info, strings):
    assert info['CFBundleIdentifier'] == 'org.zerodenet.znetsink'
    assert info['MinimumOSVersion'] == '16.4', 'iOS application floor changed'
    assert info['CFBundlePackageType'] == 'APPL', 'not a complete application'
    assert re.fullmatch(r'\d+\.\d+\.\d+', info['CFBundleShortVersionString'])
    expected = 'iPhoneSimulator' if sdk == 'iphonesimulator' else 'iPhoneOS'
    assert expected in info['CFBundleSupportedPlatforms'], 'wrong Apple SDK'
    assert set(arches) <= {'arm64', 'x86_64'} and arches
    if sdk == 'iphoneos':
        assert arches == ['arm64'] and re.search(r'platform\s+IOS\b', build_info)
    else:
        assert re.search(r'platform\s+IOSSIMULATOR\b', build_info)
    assert '_start_app' in symbols, 'Rust mobile entry point is absent'
    assert 'init_plugin_znet_native_io' in symbols, 'Swift plugin binding absent'
    assert '/index.html' in strings and '/_app/immutable/' in strings, 'embedded frontend absent'
    assert 'http://localhost:1420' not in strings, 'development server leaked into packaged host'


def command(*args):
    return subprocess.check_output(args, text=True, errors='strict')


def validate_signature(sdk, returncode, details):
    if returncode != 0:
        assert 'code object is not signed at all' in details, 'codesign inspection failed'
        return 'none'
    # Apple arm64 Simulator tools can emit an ad-hoc signature under --no-sign.
    # It contains no signing identity/certificate; never admit it for device SDK.
    assert sdk == 'iphonesimulator', 'device application must remain unsigned'
    assert re.search(r'^Signature=adhoc$', details, re.M), 'unexpected signing identity'
    assert re.search(r'^TeamIdentifier=not set$', details, re.M), 'unexpected signing team'
    assert not re.search(r'^Authority=', details, re.M), 'certificate-backed signature forbidden'
    return 'ad-hoc'


def validate_inventory_asset(data, sdk):
    assert 0 < len(data) <= 16 * 1024 * 1024, 'iOS license asset budget'
    inventory = json.loads(data)
    assert inventory['schemaVersion'] == 1 and inventory['target']['platform'] == 'ios'
    expected = ['aarch64-apple-ios'] if sdk == 'iphoneos' else ['aarch64-apple-ios-sim', 'x86_64-apple-ios']
    assert inventory['target']['triple'] in expected and inventory['target']['profile'] == 'debug'
    assert inventory['entries'] and isinstance(inventory['gaps'], list)
    assert inventory['provenance']['kind'] == 'ios-application-resolved-input-inventory'
    materials = set()
    for text in inventory['texts']:
        digest = hashlib.sha256(text['text'].encode('utf-8')).hexdigest()
        assert text['sha256'] == digest and text['id'] == 'sha256-' + digest
        materials.add(text['id'])
    for entry in inventory['entries']:
        assert all(reference in materials for reference in entry['licenseTextIds'] + entry['noticeTextIds'])
    return inventory


def public_entitlements(returncode, output, signature_kind):
    """Inspect the built App only; never print arbitrary entitlement values.

    These are diagnostic facts, not a grant or a Keychain acceptance result.
    Missing/failed inspection must remain distinguishable from an empty plist.
    """
    result = dict(state='inspection-failed', codeSignExitStatus=returncode,
                  signature=signature_kind, applicationIdentifierPresent=None,
                  applicationIdentifierMatchesBundle=None, teamIdentifierPresent=None,
                  keychainAccessGroupCount=None, applicationGroupCount=None,
                  firstKeychainGroupMatchesApplicationIdentifier=None)
    if returncode != 0:
        return result
    if not output.strip():
        result['state'] = 'absent'
        return result
    if len(output) > 64 * 1024:
        result['state'] = 'invalid'
        return result
    try:
        values = plistlib.loads(output)
        assert type(values) is dict
        application = values.get('application-identifier')
        team = values.get('com.apple.developer.team-identifier')
        assert application is None or type(application) is str
        assert team is None or type(team) is str
        groups = values.get('keychain-access-groups', [])
        shared = values.get('com.apple.security.application-groups', [])
        assert type(groups) is list and type(shared) is list
        assert len(groups) <= 256 and len(shared) <= 256
        assert all(type(value) is str and 0 < len(value) <= 1024 for value in groups + shared)
        assert all(value is None or 0 < len(value) <= 1024 for value in (application, team))
    except (ValueError, TypeError, AssertionError, plistlib.InvalidFileException, ExpatError):
        result['state'] = 'invalid'
        return result
    result.update(state='inspected', applicationIdentifierPresent=application is not None,
                  applicationIdentifierMatchesBundle=(application.endswith('.org.zerodenet.znetsink')
                      if application is not None else None),
                  teamIdentifierPresent=team is not None, keychainAccessGroupCount=len(groups),
                  applicationGroupCount=len(shared),
                  firstKeychainGroupMatchesApplicationIdentifier=(groups[0] == application
                      if groups and application is not None else None))
    return result


def package(args):
    candidates = []
    for app in pathlib.Path(args.root).rglob('*.app'):
        if app.is_dir() and (app / 'Info.plist').is_file():
            info = plistlib.loads((app / 'Info.plist').read_bytes())
            expected = 'iPhoneSimulator' if args.sdk == 'iphonesimulator' else 'iPhoneOS'
            if expected in info.get('CFBundleSupportedPlatforms', []) and info.get('CFBundleIdentifier') == 'org.zerodenet.znetsink':
                candidates.append((app, info))
    # CLI can retain an archive and export: require their executables to agree.
    assert candidates, 'no complete main App produced by locked Tauri CLI'
    digests = {hashlib.sha256((app / info['CFBundleExecutable']).read_bytes()).hexdigest() for app, info in candidates}
    assert len(digests) == 1, 'ambiguous/stale application products'
    app, info = candidates[0]
    executable = app / info['CFBundleExecutable']
    assert executable.parent == app and executable.is_file()
    arches = command('xcrun', 'lipo', '-archs', str(executable)).split()
    symbols = command('xcrun', 'nm', '-a', str(executable))
    build_info = command('xcrun', 'vtool', '-show-build', str(executable))
    embedded = command('strings', str(executable))
    validate_package(info, args.sdk, arches, symbols, build_info, embedded)
    license_bytes = (app / 'assets/license-inventory.native.json').read_bytes()
    inventory = validate_inventory_asset(license_bytes, args.sdk)
    assert 'ZNET_IOS_UI_SMOKE_NONCE' in embedded, 'Debug native smoke code absent from complete App'
    proof = pathlib.Path(args.copy)
    (proof / 'macho-build.txt').write_text(build_info)
    (proof / 'native-binding-symbols.txt').write_text('\n'.join(line for line in symbols.splitlines() if '_start_app' in line or 'init_plugin_znet_native_io' in line) + '\n')
    (proof / 'app-info.json').write_text(json.dumps(info, indent=2) + '\n')
    assert not (app / 'embedded.mobileprovision').exists(), 'provisioning must remain absent'
    signature = subprocess.run(['codesign', '-dv', '--verbose=4', str(app)], capture_output=True, text=True)
    details = signature.stdout + signature.stderr
    (proof / 'codesign-details.txt').write_text(details)
    signature_kind = validate_signature(args.sdk, signature.returncode, details)
    # Apple DTS requires built-product entitlements, not the project input file.
    # Read-only: no re-signing, credentials, team identifiers or fallback groups.
    entitlements = subprocess.run(['codesign', '-d', '--entitlements', '-', str(app)],
                                 capture_output=True)
    entitlement_summary = public_entitlements(entitlements.returncode, entitlements.stdout,
                                              signature_kind)
    (proof / 'built-entitlements-summary.json').write_text(json.dumps(entitlement_summary, indent=2) + '\n')
    destination = pathlib.Path(args.copy) / 'ZNetSink.app'
    shutil.copytree(app, destination)
    receipt = dict(schemaVersion=1, commit=command('git', 'rev-parse', 'HEAD').strip(),
                   sdk=args.sdk, architectures=arches, minimumSystemVersion=info['MinimumOSVersion'],
                   applicationId=info['CFBundleIdentifier'], binarySha256=next(iter(digests)),
                   unsigned=signature_kind == 'none', signature=signature_kind,
                   signingIdentity='none', noSigningCredentials=True,
                   builtEntitlements=entitlement_summary,
                   frontend='embedded-assets', nativePluginBinding=True,
                   deviceInstalled=False, vpnAccepted=False, executionVerified=False,
                   nativeLicenseStatus='available', nativeLicenseEntries=len(inventory['entries']),
                   nativeLicenseGaps=len(inventory['gaps']), nativeLicenseSha256=hashlib.sha256(license_bytes).hexdigest(),
                   app=str(destination.resolve()))
    pathlib.Path(args.output).write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(receipt))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('action', choices=['package'])
    parser.add_argument('--sdk', required=True, choices=['iphonesimulator', 'iphoneos'])
    parser.add_argument('--root', required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--copy', required=True)
    package(parser.parse_args())
