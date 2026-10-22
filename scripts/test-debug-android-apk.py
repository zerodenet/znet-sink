#!/usr/bin/env python3
"""Parser/rejection tests using fake Android tools, NOT real signature verification."""
import importlib.util
import json
import pathlib
import struct
import tempfile
import unittest
import warnings
import zipfile
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("debug_apk_verifier", pathlib.Path(__file__).with_name("verify-debug-android-apk.py"))
VERIFIER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFIER)
SHA = "1" * 40
CERT = "ab" * 32
SIGNATURE = "\n".join(["Verifies", "Verified using v1 scheme (JAR signing): false", "Verified using v2 scheme (APK Signature Scheme v2): true", "Verified using v3 scheme (APK Signature Scheme v3): false", "Number of signers: 1", "Signer #1 certificate SHA-256 digest: " + CERT])
BADGING = "\n".join(["package: name='org.zerodenet.znetsink.debug' versionCode='1' versionName='0.0.3'", "sdkVersion:'24'", "targetSdkVersion:'36'", "launchable-activity: name='org.zerodenet.znetsink.MainActivity' label='ZNet Sink' icon=''", "native-code: 'arm64-v8a'"])
MANIFEST = "\n".join(["N: android=http://schemas.android.com/apk/res/android", "E: manifest (line=1)", "  E: application (line=5)", "    A: android:debuggable(0x0101000f)=(type 0x12)0xffffffff", "    E: activity (line=6)"])


def native(abi="arm64-v8a"):
    target = VERIFIER.target_for_abi(abi)
    header = bytearray(target["elfHeaderBytes"])
    header[:6] = b"\x7fELF" + bytes([target["elfClass"], 1])
    struct.pack_into("<HH", header, 16, 3, target["elfMachine"])
    return bytes(header) + b"/index.html\x00/_app/immutable/entry/app.hash.js\x00/_app/immutable/entry/start.hash.js\x00/_app/immutable/nodes/0.hash.js\x00/_app/immutable/nodes/2.hash.js"


class DebugApkTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = pathlib.Path(self.temp.name)
        self.apk = self.root / "test-debug.apk"
        self.receipt = self.root / "receipt.json"
        self.tools = self.root / "build-tools"
        self.tools.mkdir()
        self.entries = {"AndroidManifest.xml": b"binary manifest placeholder", "classes.dex": b"\x00".join(VERIFIER.ENTRY_POINTS), "lib/arm64-v8a/libgui_lib.so": native(), "assets/tauri.conf.json": json.dumps({"identifier": VERIFIER.TAURI_IDENTIFIER, "build": {"devUrl": None}}).encode()}
        self.write_zip()
        self.tool_outputs = {"signature": SIGNATURE, "badging": BADGING, "manifest": MANIFEST, "exit": 0}
        self.write_tools()

    def write_zip(self):
        with zipfile.ZipFile(self.apk, "w", compression=zipfile.ZIP_STORED) as archive:
            for name, data in self.entries.items():
                archive.writestr(name, data)

    def write_tools(self):
        config = self.root / "tool-fixture.json"
        config.write_text(json.dumps(self.tool_outputs))
        # The fake executables prove invocation/argument and failure handling only.
        # They deliberately do not sign the synthetic ZIP fixture.
        script = "#!/usr/bin/env python3\nimport json,pathlib,sys\nroot=pathlib.Path(__file__).parent.parent\nf=json.loads((root/'tool-fixture.json').read_text())\nwith (root/'calls.jsonl').open('a') as out: out.write(json.dumps([pathlib.Path(__file__).name]+sys.argv[1:])+'\\n')\nif pathlib.Path(__file__).name=='apksigner': print(f['signature'])\nelif sys.argv[2]=='badging': print(f['badging'])\nelse:\n print(f['manifest'])\n if f.get('mutate'): \n  with pathlib.Path(sys.argv[3]).open('ab') as apk: apk.write(b'CHANGED')\nsys.exit(f['exit'])\n"
        for name in ["aapt", "apksigner"]:
            path = self.tools / name
            path.write_text(script)
            path.chmod(0o755)

    def verify(self):
        return VERIFIER.verify_debug(self.apk, self.tools, SHA, self.receipt)

    def denied(self):
        with self.assertRaises((ValueError, zipfile.BadZipFile)):
            self.verify()
        self.assertFalse(self.receipt.exists())

    def test_fake_tools_accept_metadata_only_and_execute_required_real_subprocess_commands(self):
        result = self.verify()
        calls = [json.loads(line) for line in (self.root / "calls.jsonl").read_text().splitlines()]
        self.assertEqual(calls, [["apksigner", "verify", "--verbose", "--print-certs", "--min-sdk-version", "24", str(self.apk)], ["aapt", "dump", "badging", str(self.apk)], ["aapt", "dump", "xmltree", str(self.apk), "AndroidManifest.xml"]])
        self.assertEqual(result, json.loads(self.receipt.read_text()))
        self.assertEqual(result["sha256"], VERIFIER.digest(self.apk))
        self.assertEqual(result["bytes"], self.apk.stat().st_size)
        self.assertEqual(result["commit"], SHA)
        self.assertEqual(result["signature"]["certificateSha256"], CERT)
        self.assertEqual(result["frontend"]["evidence"], "packaged-asset-keys-only")
        self.assertFalse(result["frontend"]["executionVerified"])
        self.assertEqual(len(result["nativeEntryPoints"]), 4)

    def test_android_debug_install_identity_is_distinct_from_base_tauri_and_jni_namespace(self):
        result = self.verify()
        self.assertEqual(result["androidApplicationId"], "org.zerodenet.znetsink.debug")
        self.assertEqual(result["tauriIdentifier"], "org.zerodenet.znetsink")
        self.assertEqual(result["namespace"], "org.zerodenet.znetsink")
        self.assertEqual(result["activity"], "org.zerodenet.znetsink.MainActivity")
        self.assertNotEqual(result["androidApplicationId"], result["tauriIdentifier"])

    def x86_fixture(self):
        del self.entries["lib/arm64-v8a/libgui_lib.so"]
        self.entries["lib/x86/libgui_lib.so"] = native("x86")
        self.tool_outputs["badging"] = BADGING.replace("'arm64-v8a'", "'x86'")
        self.write_tools()
        self.write_zip()

    def test_x86_requires_explicit_expected_abi_and_preserves_debug_identity_checks(self):
        self.x86_fixture()
        self.denied()  # Default arm64 must never silently follow APK claims.
        result = VERIFIER.verify_debug(self.apk, self.tools, SHA, self.receipt, "x86")
        self.assertEqual(result["abi"], "x86")
        self.assertEqual(result["targetTriple"], "i686-linux-android")
        self.assertEqual(result["package"]["abis"], ["x86"])
        self.assertEqual(result["package"]["minimumSdk"], 24)
        self.assertFalse(result["frontend"]["executionVerified"])

    def test_x86_path_cannot_hide_wrong_elf_class_machine_type_or_truncation(self):
        self.x86_fixture()
        valid = native("x86")
        wrong_machine = bytearray(valid)
        struct.pack_into("<H", wrong_machine, 18, 183)
        wrong_type = bytearray(valid)
        struct.pack_into("<H", wrong_type, 16, 2)
        for value in [native(), bytes(wrong_machine), bytes(wrong_type), valid[:51],
                      valid[:5] + b"\x02" + valid[6:]]:
            self.entries["lib/x86/libgui_lib.so"] = value
            self.write_zip()
            with self.assertRaises(ValueError):
                VERIFIER.verify_debug(self.apk, self.tools, SHA, self.receipt, "x86")
            self.assertFalse(self.receipt.exists())

    def test_x86_badging_and_every_native_library_must_match_the_expected_abi(self):
        self.x86_fixture()
        self.tool_outputs["badging"] = BADGING
        self.write_tools()
        with self.assertRaises(ValueError):
            VERIFIER.verify_debug(self.apk, self.tools, SHA, self.receipt, "x86")
        self.tool_outputs["badging"] = BADGING.replace("'arm64-v8a'", "'x86'")
        self.write_tools()
        self.entries["lib/x86/another.so"] = native()
        self.write_zip()
        with self.assertRaises(ValueError):
            VERIFIER.verify_debug(self.apk, self.tools, SHA, self.receipt, "x86")
        self.entries["lib/x86/another.so"] = native("x86")
        self.entries["lib/arm64-v8a/another.so"] = native()
        self.write_zip()
        with self.assertRaises(ValueError):
            VERIFIER.verify_debug(self.apk, self.tools, SHA, self.receipt, "x86")
        self.assertFalse(self.receipt.exists())

    def test_unadapted_expected_abi_is_rejected(self):
        for abi in ["x86_64", "armeabi-v7a", "", "../x86"]:
            with self.assertRaises(ValueError):
                VERIFIER.verify_debug(self.apk, self.tools, SHA, self.receipt, abi)

    def test_unsigned_or_failed_signature_command_never_publishes_receipt(self):
        self.tool_outputs["exit"] = 1
        self.write_tools()
        self.denied()

    def test_no_verification_or_signature_scheme_or_cert_is_rejected(self):
        for bad in [SIGNATURE.replace("Verifies", "DOES NOT VERIFY"), SIGNATURE.replace("true", "false"), SIGNATURE.replace(CERT, "aabb"), SIGNATURE + "\nSigner #2 certificate SHA-256 digest: " + CERT, SIGNATURE.replace("Number of signers: 1", "Number of signers: 2")]:
            with self.subTest(output=bad[:40]):
                with self.assertRaises(ValueError):
                    VERIFIER.verify_signature(bad)

    def test_wrong_release_id_wrong_api_or_wrong_launchable_activity_is_rejected(self):
        for original, replacement in [(VERIFIER.APP_ID, "org.zerodenet.znetsink"), ("sdkVersion:'24'", "sdkVersion:'23'"), ("targetSdkVersion:'36'", "targetSdkVersion:'35'"), (VERIFIER.ACTIVITY, ".MainActivity"), ("native-code: 'arm64-v8a'", "native-code: 'arm64-v8a' 'x86_64'")]:
            with self.subTest(replacement=replacement):
                with self.assertRaises(ValueError):
                    VERIFIER.verify_badging(BADGING.replace(original, replacement))

    def test_duplicate_or_missing_package_proof_is_rejected(self):
        for value in [BADGING + "\nsdkVersion:'24'", BADGING.replace("launchable-activity:", "unlaunchable-activity:")]:
            with self.assertRaises(ValueError):
                VERIFIER.verify_badging(value)

    def test_manifest_requires_typed_true_on_exact_application_attribute(self):
        for value in [MANIFEST.replace("0xffffffff", "0x0"), MANIFEST.replace("(type 0x12)0xffffffff", '"true"'), MANIFEST.replace("android:debuggable", "android:enabled"), MANIFEST.replace("0x0101000f", "0x12345678"), MANIFEST.replace("    A: android:debuggable", "      A: android:debuggable"), MANIFEST + "\n  E: application (line=10)"]:
            with self.subTest(manifest=value):
                with self.assertRaises(ValueError):
                    VERIFIER.verify_debuggable(value)

    def test_nested_debuggable_does_not_authorize_non_debug_application(self):
        value = MANIFEST.replace("0xffffffff", "0x0") + "\n      A: android:debuggable(0x0101000f)=(type 0x12)0xffffffff"
        with self.assertRaises(ValueError):
            VERIFIER.verify_debuggable(value)

    def test_malformed_zip_and_crc_damage_are_rejected(self):
        self.apk.write_bytes(b"not an APK")
        self.denied()
        self.write_zip()
        data = self.apk.read_bytes()
        self.apk.write_bytes(data.replace(b"binary manifest placeholder", b"Binary manifest placeholder", 1))
        self.denied()

    def test_duplicate_names_are_rejected_even_when_last_entry_looks_valid(self):
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", UserWarning)
            with zipfile.ZipFile(self.apk, "a") as archive:
                archive.writestr("classes.dex", b"\x00".join(VERIFIER.ENTRY_POINTS))
        self.denied()

    def test_native_missing_wrong_abi_or_wrong_elf_is_rejected(self):
        for mode in ["missing", "extra-abi", "wrong-elf"]:
            with self.subTest(mode=mode):
                original = self.entries.copy()
                if mode == "missing": del self.entries["lib/arm64-v8a/libgui_lib.so"]
                elif mode == "extra-abi": self.entries["lib/x86_64/libgui_lib.so"] = native()
                else: self.entries["lib/arm64-v8a/libgui_lib.so"] = b"not ELF"
                self.write_zip()
                self.denied()
                self.entries = original

    def test_entry_points_may_be_split_across_real_multidex_filenames(self):
        self.entries["classes.dex"] = VERIFIER.ENTRY_POINTS[0]
        self.entries["classes2.dex"] = VERIFIER.ENTRY_POINTS[1]
        self.entries["classes10.dex"] = b"\x00".join(VERIFIER.ENTRY_POINTS[2:])
        self.write_zip()
        self.assertTrue(self.verify()["nativeDocumentsPackaged"])

    def test_each_native_document_entry_point_is_required(self):
        for missing in VERIFIER.ENTRY_POINTS:
            self.entries["classes.dex"] = b"\x00".join(entry for entry in VERIFIER.ENTRY_POINTS if entry != missing)
            self.write_zip()
            self.denied()

    def test_embedded_frontend_requires_index_and_production_page_and_entry_keys(self):
        for missing in [b"/index.html", b"/_app/immutable/entry/app.hash.js", b"/_app/immutable/entry/start.hash.js", b"/_app/immutable/nodes/2.hash.js"]:
            self.entries["lib/arm64-v8a/libgui_lib.so"] = native().replace(missing, b"")
            self.write_zip()
            self.denied()

    def test_packaged_tauri_id_and_development_url_are_checked(self):
        for config in [{"identifier": VERIFIER.APP_ID, "build": {}}, {"identifier": VERIFIER.TAURI_IDENTIFIER, "build": {"devUrl": "http://localhost:1420/"}}, {"identifier": VERIFIER.TAURI_IDENTIFIER}, []]:
            self.entries["assets/tauri.conf.json"] = json.dumps(config).encode()
            self.write_zip()
            self.denied()

    def test_apk_change_during_tools_rejects_without_receipt(self):
        self.tool_outputs["mutate"] = True
        self.write_tools()
        self.denied()

    def test_hash_change_is_checked_even_if_file_identity_is_unchanged(self):
        original = VERIFIER.digest(self.apk)
        with patch.object(VERIFIER, "digest", side_effect=[original, "ff" * 32]):
            self.denied()

    def test_failed_verification_preserves_existing_receipt_bytes(self):
        self.receipt.write_bytes(b"prior completed receipt")
        self.tool_outputs["exit"] = 1
        self.write_tools()
        with self.assertRaises(ValueError):
            self.verify()
        self.assertEqual(self.receipt.read_bytes(), b"prior completed receipt")

    def test_invalid_commit_and_receipt_alias_are_rejected(self):
        for sha in ["main", "1" * 39, "G" * 40]:
            with self.assertRaises(ValueError):
                VERIFIER.verify_debug(self.apk, self.tools, sha, self.receipt)
        with self.assertRaises(ValueError):
            VERIFIER.verify_debug(self.apk, self.tools, SHA, self.apk)

    def test_unsafe_paths_and_symlink_entries_reject(self):
        for name, symlink in [("../outside", False), ("assets/link", True)]:
            self.write_zip()
            with zipfile.ZipFile(self.apk, "a") as archive:
                entry = zipfile.ZipInfo(name)
                if symlink: entry.external_attr = (0o120777 << 16)
                archive.writestr(entry, b"target")
            self.denied()


if __name__ == "__main__":
    unittest.main()
