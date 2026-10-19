import copy
import pathlib
import unittest
from importlib.machinery import SourceFileLoader

verify = SourceFileLoader('verify_ios_host', str(pathlib.Path(__file__).with_name('verify-ios-host.py'))).load_module()


class HostContracts(unittest.TestCase):
    def setUp(self):
        self.info = dict(CFBundleIdentifier='org.zerodenet.znetsink', MinimumOSVersion='16.4',
                         CFBundlePackageType='APPL', CFBundleShortVersionString='0.0.3',
                         CFBundleSupportedPlatforms=['iPhoneSimulator'])
        self.checks = dict(rustIpc=True, nativePresentation=True, fourTabs=True, onboarding=True,
                          aboutAndReturn=True, vpnUnsupported=True, nativeLicenseUnavailable=True,
                          applicationId='org.zerodenet.znetsink', width=393, height=852)
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


if __name__ == '__main__':
    unittest.main()
