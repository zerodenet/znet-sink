import copy
import json
import pathlib
import plistlib
import unittest
from importlib.machinery import SourceFileLoader

verify = SourceFileLoader('verify_ios_host', str(pathlib.Path(__file__).with_name('verify-ios-host.py'))).load_module()
smoke = SourceFileLoader('smoke_ios_host', str(pathlib.Path(__file__).with_name('smoke-ios-host.py'))).load_module()


class HostContracts(unittest.TestCase):
    def test_built_entitlements_are_bounded_public_facts_not_signing_or_keychain_acceptance(self):
        secret = 'SECRET-CANARY'
        values = {'application-identifier': secret + '.org.zerodenet.znetsink',
                  'com.apple.developer.team-identifier': secret,
                  'keychain-access-groups': [secret + '.org.zerodenet.znetsink'],
                  'com.apple.security.application-groups': ['group.' + secret],
                  'future-private-field': secret}
        result = verify.public_entitlements(0, plistlib.dumps(values), 'ad-hoc')
        self.assertEqual(result['state'], 'inspected')
        self.assertTrue(result['applicationIdentifierMatchesBundle'])
        self.assertTrue(result['firstKeychainGroupMatchesApplicationIdentifier'])
        self.assertEqual(result['keychainAccessGroupCount'], 1)
        self.assertNotIn(secret, json.dumps(result))
        self.assertNotIn('accepted', json.dumps(result).lower())
        empty = verify.public_entitlements(0, plistlib.dumps({}), 'ad-hoc')
        self.assertEqual(empty['keychainAccessGroupCount'], 0)
        self.assertFalse(empty['applicationIdentifierPresent'])
        self.assertIsNone(empty['applicationIdentifierMatchesBundle'])
        for code, data, expected in [(1, b'', 'inspection-failed'), (0, b'', 'absent'),
                                     (0, b'x'*65537, 'invalid'), (0, b'not a plist', 'invalid'),
                                     (0, b'<?xml version="1.0"?><plist><dict><key>broken', 'invalid'),
                                     (0, plistlib.dumps({'keychain-access-groups':'secret'}), 'invalid'),
                                     (0, plistlib.dumps({'application-identifier':False}), 'invalid')]:
            with self.subTest(code=code, expected=expected):
                result=verify.public_entitlements(code, data, 'none')
                self.assertEqual(result['state'], expected)
                self.assertIsNone(result['keychainAccessGroupCount'])

    def test_isolated_positive_diagnostics_do_not_promote_unavailable_storage(self):
        receipt=copy.deepcopy(self.receipt)
        receipt['checks'].update(nativeSecureStorageState='unavailable',nativeSecureStorageAccepted=False)
        def step(status): return dict(attempted=True,osStatus=status)
        value=receipt['checks']['nativeKeychainDiagnostics']
        value.update(protectedDataAvailable=True,stopStage='completed',roundTrip=True,readBackMatches=True,
                     lastHostAdmission=dict(protectedDataAvailable=True,probe=step(-50),apiAvailable=False),
                     availabilityProbe=step(-50),readBeforeCreate=step(-25300),random=step(0),
                     create=step(0),readAfterCreate=step(0),cleanup=step(0))
        verify.validate_smoke(receipt,'a'*32)
        self.assertFalse(receipt['checks']['nativeSecureStorageAccepted'])
        value['protectedDataAvailable']=False
        value['lastHostAdmission']=dict(protectedDataAvailable=False,
            probe=dict(attempted=False,osStatus=None),apiAvailable=False)
        verify.validate_smoke(receipt,'a'*32)
        self.assertFalse(receipt['checks']['nativeSecureStorageAccepted'])
        for key in ('keyBase64','account','url'):
            altered=copy.deepcopy(value);altered[key]='secret-canary'
            with self.assertRaises(AssertionError): verify.validate_keychain_diagnostics(altered)
        altered=copy.deepcopy(value);altered['create']['attempted']=False
        with self.assertRaises(AssertionError): verify.validate_keychain_diagnostics(altered)
    def test_progress_is_nonce_scoped_bounded_metadata_and_never_ui_completion(self):
        value=dict(schemaVersion=1,nonce='a'*32,bundleIdentifier='org.zerodenet.znetsink',pid=42,
                   stage='waiting-for-document',documentScheme='tauri',documentIsLoading=True,
                   url='https://private/?token=secret',payload='secret')
        self.assertEqual(verify.public_startup_progress(value,'a'*32),
                         dict(stage='waiting-for-document',pid=42,documentScheme='tauri',documentIsLoading=True))
        self.assertNotIn('secret',json.dumps(verify.public_startup_progress(value,'a'*32)))
        for patch in [dict(nonce='b'*32),dict(stage='secret'),dict(pid=True),dict(documentScheme='secret'),dict(documentIsLoading='secret')]:
            with self.subTest(patch=patch),self.assertRaises((AssertionError,KeyError)):
                verify.public_startup_progress(dict(value,**patch),'a'*32)
        with self.assertRaises((AssertionError,KeyError)): verify.validate_smoke(value,'a'*32)

    def test_lifecycle_console_projection_excludes_all_unselected_native_fields(self):
        receipts = [dict(schemaVersion=1, nonce='a'*32, pid=42, executorAccepting=phase=='ready',
                         token='private-token', platform=dict(state=phase, target=dict(os='ios'),
                         generation=str(index+1), credential='private-credential'))
                    for index, phase in enumerate(['ready', 'suspended', 'ready'])]
        projected = verify.public_plugin_lifecycle(receipts, 'a'*32)
        self.assertEqual([row['generation'] for row in projected], ['1','2','3'])
        self.assertEqual(set(projected[0]), {'pid','phase','generation','executorAccepting'})
        self.assertNotIn('private', json.dumps(projected))
        with self.assertRaises(AssertionError): verify.public_plugin_lifecycle(receipts, 'b'*32)

    def test_unavailable_secure_storage_never_counts_as_native_acceptance(self):
        receipt=copy.deepcopy(self.receipt)
        receipt['checks'].update(nativeSecureStorageState='unavailable',nativeSecureStorageAccepted=False)
        verify.validate_smoke(receipt,'a'*32)
        receipt['checks']['nativeSecureStorageAccepted']=True
        with self.assertRaises(AssertionError): verify.validate_smoke(receipt,'a'*32)
    def test_lifecycle_evidence_requires_native_transition_and_same_process(self):
        receipts = [dict(schemaVersion=1, nonce='a' * 32, pid=42, executorAccepting=phase == 'ready', platform=dict(state=phase, target=dict(os='ios'), generation=str(i + 1))) for i, phase in enumerate(['ready', 'suspended', 'ready'])]
        verify.validate_plugin_lifecycle(receipts, 'a' * 32)
        for patch in [dict(pid=43), dict(executorAccepting=True), dict(nonce='b' * 32), dict(platform=dict(state='suspended', target=dict(os='ios'), generation='1'))]:
            altered = copy.deepcopy(receipts)
            altered[1].update(patch)
            with self.subTest(patch=patch), self.assertRaises(AssertionError):
                verify.validate_plugin_lifecycle(altered, 'a' * 32)
    def test_packaged_inventory_rejects_wrong_sdk_or_missing_materials(self):
        value = dict(schemaVersion=1, target=dict(platform='ios', triple='aarch64-apple-ios-sim', profile='debug'),
                     provenance=dict(kind='ios-application-resolved-input-inventory'), texts=[], gaps=[],
                     entries=[dict(licenseTextIds=[], noticeTextIds=[])])
        verify.validate_inventory_asset(json.dumps(value).encode(), 'iphonesimulator')
        with self.assertRaises(AssertionError):
            verify.validate_inventory_asset(json.dumps(value).encode(), 'iphoneos')
        value['entries'][0]['licenseTextIds'] = ['missing']
        with self.assertRaises(AssertionError):
            verify.validate_inventory_asset(json.dumps(value).encode(), 'iphonesimulator')
    def test_runtime_discovery_uses_reported_versions_and_available_phones(self):
        def runtime(version, available=True):
            return dict(identifier='com.apple.CoreSimulator.SimRuntime.iOS-' + version.replace('.', '-'),
                        version=version, isAvailable=available)
        older, newer, unavailable = runtime('18.5'), runtime('26.2'), runtime('27.0', False)
        phone = dict(name='iPhone fixture', isAvailable=True, udid='fixture-only')
        devices = {entry['identifier']: [phone] for entry in [older, newer, unavailable]}
        selected, phones = smoke.select_runtime([older, unavailable, newer], devices)
        self.assertEqual(selected, newer)
        self.assertEqual(phones, [phone])
        with self.assertRaises(AssertionError):
            smoke.select_runtime([unavailable], devices)
        with self.assertRaises(AssertionError):
            smoke.select_runtime([newer], {newer['identifier']: [dict(phone, isAvailable=False)]})

    def setUp(self):
        self.info = dict(CFBundleIdentifier='org.zerodenet.znetsink', MinimumOSVersion='16.4',
                         CFBundlePackageType='APPL', CFBundleShortVersionString='0.0.3',
                         CFBundleSupportedPlatforms=['iPhoneSimulator'])
        self.checks = dict(rustIpc=True, nativePresentation=True, fourTabs=True, onboarding=True,
                          aboutAndReturn=True, vpnUnsupported=True, nativeLicenseAvailable=True, nativePluginVm=True,
                          nativeSecureStorageContract=True, nativeSecureStorageAccepted=True, nativeSecureStorageState='available',
                          applicationId='org.zerodenet.znetsink', width=393, height=852)
        skipped=dict(attempted=False,osStatus=None)
        self.checks['nativeKeychainDiagnostics']=dict(schemaVersion=1,isolated=True,
            protectedDataAvailable=False,lastHostAdmission=None,readBackMatches=None,
            stopStage='protectedDataGate',roundTrip=False,
            **{key:dict(skipped) for key in ('availabilityProbe','readBeforeCreate','random','create','readAfterCreate','cleanup')})
        self.receipt = dict(schemaVersion=1, nonce='a' * 32, status='completed',
                            bundleIdentifier='org.zerodenet.znetsink', nativePlugin='znet-native-io',
                            webViewInspectable=True, checks=self.checks)

    def validate(self, **patch):
        args = dict(info=self.info, sdk='iphonesimulator', arches=['arm64'],
                    symbols='_start_app init_plugin_znet_native_io',
                    build_info='platform IOSSIMULATOR', strings='/index.html /_app/immutable/entry/app.js')
        args.update(patch)
        verify.validate_package(**args)

    def test_actual_package_fields_distinguish_simulator_and_device(self):
        self.validate()
        device = copy.deepcopy(self.info)
        device['CFBundleSupportedPlatforms'] = ['iPhoneOS']
        self.validate(info=device, sdk='iphoneos', build_info='platform IOS')
        with self.assertRaises(AssertionError):
            self.validate(info=device)
        with self.assertRaises(AssertionError):
            self.validate(info=device, sdk='iphoneos', build_info='platform IOSSIMULATOR')

    def test_library_or_wrong_app_cannot_pass_as_complete_host(self):
        for key, value in [('CFBundlePackageType', 'FMWK'), ('CFBundleIdentifier', 'fixture.other'),
                           ('MinimumOSVersion', '15.0'), ('CFBundleShortVersionString', '0.0.3-dev.test')]:
            info = dict(self.info, **{key: value})
            with self.subTest(key=key), self.assertRaises(AssertionError):
                self.validate(info=info)

    def test_missing_rust_or_swift_or_frontend_never_passes(self):
        for patch in [dict(symbols='_start_app'), dict(symbols='init_plugin_znet_native_io'),
                      dict(strings='/index.html'), dict(strings='/index.html /_app/immutable/a.js http://localhost:1420')]:
            with self.subTest(patch=patch), self.assertRaises(AssertionError):
                self.validate(**patch)

    def test_smoke_requires_original_nonce_and_all_observed_checks(self):
        verify.validate_smoke(self.receipt, 'a' * 32)
        for key in self.checks:
            if isinstance(self.checks[key], bool):
                receipt = copy.deepcopy(self.receipt)
                receipt['checks'][key] = False
                with self.subTest(key=key), self.assertRaises(AssertionError):
                    verify.validate_smoke(receipt, 'a' * 32)
        with self.assertRaises(AssertionError):
            verify.validate_smoke(self.receipt, 'b' * 32)

    def test_failed_or_uninspectable_native_webview_cannot_claim_success(self):
        for patch in [dict(status='failed'), dict(webViewInspectable=False), dict(nativePlugin='fixture')]:
            with self.subTest(patch=patch), self.assertRaises(AssertionError):
                verify.validate_smoke(dict(self.receipt, **patch), 'a' * 32)

    def test_only_simulator_can_have_certificate_free_ad_hoc_signature(self):
        unsigned = 'App: code object is not signed at all\n'
        adhoc = 'Signature=adhoc\nTeamIdentifier=not set\nCodeDirectory flags=0x20002(adhoc,linker-signed)\n'
        self.assertEqual(verify.validate_signature('iphoneos', 1, unsigned), 'none')
        self.assertEqual(verify.validate_signature('iphonesimulator', 0, adhoc), 'ad-hoc')
        for sdk, status, details in [('iphoneos', 0, adhoc), ('iphonesimulator', 1, 'tool failed'),
                                     ('iphonesimulator', 0, adhoc.replace('not set', 'TEAM')),
                                     ('iphonesimulator', 0, adhoc + 'Authority=Certificate\n'),
                                     ('iphonesimulator', 0, 'Signature=unknown\nTeamIdentifier=not set\n')]:
            with self.subTest(sdk=sdk, details=details), self.assertRaises(AssertionError):
                verify.validate_signature(sdk, status, details)


if __name__ == '__main__':
    unittest.main()
