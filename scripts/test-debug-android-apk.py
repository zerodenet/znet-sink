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
import hashlib
import zlib
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("debug_apk_verifier", pathlib.Path(__file__).with_name("verify-debug-android-apk.py"))
VERIFIER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFIER)
UNSIGNED_SPEC = importlib.util.spec_from_file_location("unsigned_apk_verifier", pathlib.Path(__file__).with_name("verify-unsigned-android-apk.py"))
UNSIGNED = importlib.util.module_from_spec(UNSIGNED_SPEC)
UNSIGNED_SPEC.loader.exec_module(UNSIGNED)
SHA = "1" * 40
CERT = "ab" * 32
SIGNATURE = "\n".join(["Verifies", "Verified using v1 scheme (JAR signing): false", "Verified using v2 scheme (APK Signature Scheme v2): true", "Verified using v3 scheme (APK Signature Scheme v3): false", "Number of signers: 1", "Signer #1 certificate SHA-256 digest: " + CERT])
BADGING = "\n".join(["package: name='org.zerodenet.znetsink.debug' versionCode='1' versionName='0.0.3'", "sdkVersion:'24'", "targetSdkVersion:'36'", "launchable-activity: name='org.zerodenet.znetsink.MainActivity' label='ZNet Sink' icon=''", "native-code: 'arm64-v8a'"])
MANIFEST = "\n".join([
    "N: android=http://schemas.android.com/apk/res/android", "E: manifest (line=1)",
    '  A: package="org.zerodenet.znetsink.debug"',
    '  E: uses-permission (line=2)', '    A: android:name(0x01010003)="android.permission.INTERNET"',
    '  E: uses-permission (line=3)', '    A: android:name(0x01010003)="android.permission.RECEIVE_BOOT_COMPLETED"',
    '  E: uses-permission (line=4)', '    A: android:name(0x01010003)="android.permission.REQUEST_INSTALL_PACKAGES"',
    "  E: application (line=5)", "    A: android:debuggable(0x0101000f)=(type 0x12)0xffffffff",
    "    E: activity (line=6)", '      A: android:name(0x01010003)="org.zerodenet.znetsink.MainActivity"',
] + [line for service in VERIFIER.JOB_SERVICES for line in (
    '    E: service (line=7)', f'      A: android:name(0x01010003)="{service}"',
    '      A: android:permission(0x01010006)="android.permission.BIND_JOB_SERVICE"',
    '      A: android:exported(0x01010010)=(type 0x12)0xffffffff',
)] + [line for receiver in VERIFIER.UPDATE_RECEIVERS for line in (
    '    E: receiver (line=8)', f'      A: android:name(0x01010003)="{receiver}"',
    '      A: android:exported(0x01010010)=(type 0x12)0x0',
)] + ['      E: intent-filter (line=9)', '        E: action (line=10)',
     '          A: android:name(0x01010003)="android.intent.action.MY_PACKAGE_REPLACED"'])


def native(abi="arm64-v8a"):
    target = VERIFIER.target_for_abi(abi)
    header = bytearray(target["elfHeaderBytes"])
    header[:6] = b"\x7fELF" + bytes([target["elfClass"], 1])
    struct.pack_into("<HH", header, 16, 3, target["elfMachine"])
    elf64 = target["elfClass"] == 2
    symbol_format = "<IBBHQQ" if elf64 else "<IIIBBH"
    section_format = "<IIQQQQIIQQ" if elf64 else "<IIIIIIIIII"
    strings = b"\0" + b"\0".join(name.encode() for name in VERIFIER.JNI_ENTRY_POINTS) + b"\0"
    symbols = bytes(struct.calcsize(symbol_format))
    for index, name in enumerate(VERIFIER.JNI_ENTRY_POINTS):
        name_offset = strings.index(name.encode())
        symbol = (name_offset, 0x12, 0, 3, 0x1000 + index * 4, 4) if elf64 else (name_offset, 0x1000 + index * 4, 4, 0x12, 0, 3)
        symbols += struct.pack(symbol_format, *symbol)
    symbol_offset, string_offset = len(header), len(header) + len(symbols)
    text_offset = string_offset + len(strings)
    text = bytes(16)
    assets = b"/index.html\x00/_app/immutable/entry/app.hash.js\x00/_app/immutable/entry/start.hash.js\x00/_app/immutable/nodes/0.hash.js\x00/_app/immutable/nodes/2.hash.js"
    section_offset = text_offset + len(text) + len(assets)
    struct.pack_into("<Q" if elf64 else "<I", header, 40 if elf64 else 32, section_offset)
    struct.pack_into("<HH", header, 58 if elf64 else 46, struct.calcsize(section_format), 4)
    sections = bytes(struct.calcsize(section_format))
    for section in ((0, 11, 2, 0, symbol_offset, len(symbols), 2, 0, 4, struct.calcsize(symbol_format)),
                    (0, 3, 0, 0, string_offset, len(strings), 0, 0, 1, 0),
                    (0, 1, 6, 0x1000, text_offset, len(text), 0, 0, 4, 0)):
        sections += struct.pack(section_format, *section)
    return bytes(header) + symbols + strings + text + assets + sections


def dex(entries=None):
    entries = list(VERIFIER.ENTRY_POINTS if entries is None else entries)
    count = len(entries)
    string_offset, type_offset, class_offset = 112, 112 + count * 4, 112 + count * 8
    data_offset = class_offset + count * 32
    header = bytearray(112)
    header[:8] = b"dex\n035\0"
    struct.pack_into("<II", header, 36, 112, 0x12345678)
    struct.pack_into("<IIII", header, 56, count, string_offset, count, type_offset)
    struct.pack_into("<II", header, 96, count, class_offset)
    strings, types, definitions, data = b"", b"", b"", b""
    for index, descriptor in enumerate(entries):
        strings += struct.pack("<I", data_offset + len(data))
        types += struct.pack("<I", index)
        definitions += struct.pack("<IIIIIIII", index, 1, 0xffffffff, 0, 0xffffffff, 0, 0, 0)
        remaining, length = len(descriptor), b""
        while remaining >= 128:
            length += bytes([(remaining & 127) | 128]); remaining >>= 7
        length += bytes([remaining])
        data += length + descriptor + b"\0"
    result = header + strings + types + definitions + data
    struct.pack_into("<I", result, 32, len(result))
    result[12:32] = hashlib.sha1(result[32:]).digest()
    struct.pack_into("<I", result, 8, zlib.adler32(result[12:]) & 0xffffffff)
    return bytes(result)


class DebugApkTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = pathlib.Path(self.temp.name)
        self.apk = self.root / "test-debug.apk"
        self.receipt = self.root / "receipt.json"
        self.tools = self.root / "build-tools"
        self.tools.mkdir()
        self.entries = {"AndroidManifest.xml": b"binary manifest placeholder", "classes.dex": dex(), "lib/arm64-v8a/libgui_lib.so": native(), "assets/tauri.conf.json": json.dumps({"identifier": VERIFIER.TAURI_IDENTIFIER, "build": {"devUrl": None}}).encode()}
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
        self.assertEqual(len(result["nativeEntryPoints"]), 9)
        self.assertEqual(result["jniFunctionExports"], list(VERIFIER.JNI_ENTRY_POINTS))
        self.assertEqual(result["platformManifest"]["jobServicePermission"], "android.permission.BIND_JOB_SERVICE")
        self.assertFalse(result["platformManifest"]["executionVerified"])

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
                archive.writestr("classes.dex", dex())
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
        self.entries["classes.dex"] = dex(VERIFIER.ENTRY_POINTS[:1])
        self.entries["classes2.dex"] = dex(VERIFIER.ENTRY_POINTS[1:2])
        self.entries["classes10.dex"] = dex(VERIFIER.ENTRY_POINTS[2:])
        self.write_zip()
        self.assertTrue(self.verify()["nativeDocumentsPackaged"])

    def test_each_native_document_entry_point_is_required(self):
        for missing in VERIFIER.ENTRY_POINTS:
            self.entries["classes.dex"] = dex(entry for entry in VERIFIER.ENTRY_POINTS if entry != missing)
            self.write_zip()
            self.denied()

    def test_embedded_frontend_requires_index_and_production_page_and_entry_keys(self):
        for missing in [b"/index.html", b"/_app/immutable/entry/app.hash.js", b"/_app/immutable/entry/start.hash.js", b"/_app/immutable/nodes/2.hash.js"]:
            self.entries["lib/arm64-v8a/libgui_lib.so"] = native().replace(missing, b"x" * len(missing))
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

    def test_jni_exports_are_defined_public_functions_for_both_elf_classes(self):
        for abi in ("arm64-v8a", "x86"):
            self.assertEqual(VERIFIER.verify_jni_exports(native(abi)), list(VERIFIER.JNI_ENTRY_POINTS))
            target = VERIFIER.target_for_abi(abi)
            elf64 = target["elfClass"] == 2
            size = 24 if elf64 else 16
            first = target["elfHeaderBytes"] + size
            original = native(abi)
            mutations = []
            missing_name = bytearray(original)
            for index in range(len(VERIFIER.JNI_ENTRY_POINTS)):
                struct.pack_into("<I", missing_name, first + index * size, 0)
            mutations.append(missing_name)  # Names still occur in the string table.
            undefined = bytearray(original)
            struct.pack_into("<H", undefined, first + (6 if elf64 else 14), 0)
            mutations.append(undefined)
            hidden = bytearray(original)
            hidden[first + (5 if elf64 else 13)] = 2
            mutations.append(hidden)
            non_function = bytearray(original)
            non_function[first + (4 if elf64 else 12)] = 0x11
            mutations.append(non_function)
            invalid_strings = bytearray(original)
            offset = struct.unpack_from("<Q" if elf64 else "<I", invalid_strings, 40 if elf64 else 32)[0]
            section_size = 64 if elf64 else 40
            struct.pack_into("<I", invalid_strings, offset + section_size + (40 if elf64 else 24), 1000)
            mutations.append(invalid_strings)
            non_executable = bytearray(original)
            struct.pack_into("<Q" if elf64 else "<I", non_executable, offset + 3 * section_size + 8, 2)
            mutations.append(non_executable)
            for mutation in mutations:
                with self.subTest(abi=abi, index=mutations.index(mutation)):
                    with self.assertRaises(ValueError):
                        VERIFIER.verify_jni_exports(mutation)

    def test_dex_strings_or_references_do_not_prove_class_definitions(self):
        value = bytearray(dex())
        struct.pack_into("<II", value, 96, 0, 0)
        value[12:32] = hashlib.sha1(value[32:]).digest()
        struct.pack_into("<I", value, 8, zlib.adler32(value[12:]) & 0xffffffff)
        self.assertTrue(all(name in value for name in VERIFIER.ENTRY_POINTS))
        self.assertEqual(VERIFIER.dex_defined_entries(value), set())
        self.entries["classes.dex"] = value
        self.write_zip()
        self.denied()

    def test_dex_checksums_table_bounds_and_duplicate_definitions_are_checked(self):
        valid = dex()
        checksum_damage = bytearray(valid)
        checksum_damage[-2] ^= 1
        with self.assertRaises(ValueError):
            VERIFIER.dex_defined_entries(checksum_damage)
        for mode in ("type-index", "duplicate", "string-offset"):
            value = bytearray(valid)
            if mode == "type-index":
                offset = struct.unpack_from("<I", value, 100)[0]
                struct.pack_into("<I", value, offset, 65535)
            elif mode == "duplicate":
                offset = struct.unpack_from("<I", value, 100)[0]
                struct.pack_into("<I", value, offset + 32, 0)
            else:
                offset = struct.unpack_from("<I", value, 60)[0]
                struct.pack_into("<I", value, offset, len(value) + 1)
            value[12:32] = hashlib.sha1(value[32:]).digest()
            struct.pack_into("<I", value, 8, zlib.adler32(value[12:]) & 0xffffffff)
            with self.assertRaises(ValueError):
                VERIFIER.dex_defined_entries(value)

    def test_platform_manifest_keeps_job_system_permission_process_and_private_receivers(self):
        proof = VERIFIER.verify_platform_manifest(MANIFEST)
        self.assertEqual(proof["sameProcess"], VERIFIER.APP_ID)
        self.assertIn("no unknown-source setting changes", proof["permissionPurposes"]["android.permission.REQUEST_INSTALL_PACKAGES"])
        invalid = [
            MANIFEST.replace("android.permission.BIND_JOB_SERVICE", "android.permission.INTERNET", 1),
            MANIFEST.replace('      A: android:permission(0x01010006)="android.permission.BIND_JOB_SERVICE"', "", 1),
            MANIFEST.replace("android:exported(0x01010010)=(type 0x12)0xffffffff", "android:exported(0x01010010)=(type 0x12)0x0", 1),
            MANIFEST.replace("android:exported(0x01010010)=(type 0x12)0x0", "android:exported(0x01010010)=(type 0x12)0xffffffff", 1),
            MANIFEST.replace("android.intent.action.MY_PACKAGE_REPLACED", "android.intent.action.BOOT_COMPLETED"),
            MANIFEST.replace("android.permission.RECEIVE_BOOT_COMPLETED", "android.permission.READ_SMS"),
            MANIFEST.replace("android.permission.REQUEST_INSTALL_PACKAGES", "android.permission.INSTALL_PACKAGES"),
        ]
        for name in VERIFIER.JOB_SERVICES:
            anchor = f'      A: android:name(0x01010003)="{name}"'
            invalid += [MANIFEST.replace(anchor, anchor + '\n      A: android:process(0x01010011)=":background"'),
                        MANIFEST.replace(anchor, anchor + '\n      A: android:isolatedProcess(0x010103a9)=(type 0x12)0xffffffff'),
                        MANIFEST.replace(anchor, anchor + '\n      A: android:enabled(0x0101000e)=(type 0x12)0x0'),
                        MANIFEST.replace(anchor, anchor + '\n      A: android:directBootAware(0x01010505)=(type 0x12)0xffffffff'),
                        MANIFEST.replace(name, name + "Missing")]
        for value in invalid:
            with self.subTest(manifest=value[-180:]):
                with self.assertRaises(ValueError):
                    VERIFIER.verify_platform_manifest(value)

    def test_platform_manifest_permissions_are_scoped_to_root_and_cannot_expand(self):
        for value in (
            MANIFEST.replace("  E: application", '  E: uses-permission (line=4)\n    A: android:name(0x01010003)="android.permission.READ_CONTACTS"\n  E: application'),
            MANIFEST.replace("  E: application", '  E: uses-permission (line=4)\n    A: android:name(0x01010003)="android.permission.REQUEST_INSTALL_PACKAGES"\n  E: application'),
            MANIFEST.replace('    A: android:name(0x01010003)="android.permission.REQUEST_INSTALL_PACKAGES"', '    A: android:name(0x01010003)="android.permission.REQUEST_INSTALL_PACKAGES"\n    A: android:maxSdkVersion(0x01010271)=(type 0x10)0x17'),
            MANIFEST.replace("    A: android:debuggable", '    A: android:process(0x01010011)=":other"\n    A: android:debuggable'),
        ):
            with self.assertRaises(ValueError):
                VERIFIER.verify_platform_manifest(value)

    def unsigned_fixture(self):
        self.tool_outputs["manifest"] = MANIFEST.replace(VERIFIER.APP_ID, VERIFIER.TAURI_IDENTIFIER).replace("android:debuggable(0x0101000f)=(type 0x12)0xffffffff", "android:debuggable(0x0101000f)=(type 0x12)0x0")
        self.write_tools()

    def test_unsigned_release_reuses_defined_dex_jni_and_real_manifest_tool_contract(self):
        self.unsigned_fixture()
        result = UNSIGNED.verify_unsigned(self.apk, self.tools)
        self.assertTrue(result["unsigned"])
        self.assertEqual(result["platformManifest"]["sameProcess"], VERIFIER.TAURI_IDENTIFIER)
        self.assertEqual(result["jniFunctionExports"], list(VERIFIER.JNI_ENTRY_POINTS))
        calls = [json.loads(line) for line in (self.root / "calls.jsonl").read_text().splitlines()]
        self.assertEqual(calls, [["aapt", "dump", "xmltree", str(self.apk), "AndroidManifest.xml"]])

    def test_unsigned_release_cannot_accept_debug_manifest_or_signed_sidecar_or_v1_signature(self):
        self.unsigned_fixture()
        self.tool_outputs["manifest"] = self.tool_outputs["manifest"].replace("android:debuggable(0x0101000f)=(type 0x12)0x0", "android:debuggable(0x0101000f)=(type 0x12)0xffffffff")
        self.write_tools()
        with self.assertRaises(ValueError):
            UNSIGNED.verify_unsigned(self.apk, self.tools)
        self.unsigned_fixture()
        sidecar = self.apk.with_suffix(self.apk.suffix + ".idsig")
        sidecar.write_bytes(b"synthetic sidecar")
        with self.assertRaises(ValueError):
            UNSIGNED.verify_unsigned(self.apk, self.tools)
        sidecar.unlink()
        self.entries["META-INF/TEST.RSA"] = b"synthetic signature marker"
        self.write_zip()
        with self.assertRaises(ValueError):
            UNSIGNED.verify_unsigned(self.apk, self.tools)

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
