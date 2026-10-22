#!/usr/bin/env python3
"""Verify a build-owned, installable debug APK; never create signing material.

Tool results are package/signature evidence, not installation, UI or VPN acceptance.
Tauri 2 embeds its production frontend inside libgui_lib.so, not assets/index.html.
"""
import argparse
import hashlib
import json
import os
import pathlib
import re
import stat
import struct
import subprocess
import sys
import tempfile
import zipfile
import zlib

APP_ID = "org.zerodenet.znetsink.debug"
# Gradle adds .debug only to Android installation identity. Tauri/JNI stay base.
TAURI_IDENTIFIER = "org.zerodenet.znetsink"
NAMESPACE = "org.zerodenet.znetsink"
ACTIVITY = "org.zerodenet.znetsink.MainActivity"
ENTRY_POINTS = (
    b"Lorg/zerodenet/znetsink/MainActivity;",
    b"Lorg/zerodenet/znetsink/nativeio/NativeIoPlugin;",
    b"Lorg/zerodenet/znetsink/nativeio/NativeIoFileProvider;",
    b"Lorg/zerodenet/znetsink/nativeio/ShareSelectionReceiver;",
    b"Lorg/zerodenet/znetsink/nativeio/NativePluginJobs;",
    b"Lorg/zerodenet/znetsink/nativeio/NativePluginJobService;",
    b"Lorg/zerodenet/znetsink/nativeio/NativeAppUpdateDownloadJob;",
    b"Lorg/zerodenet/znetsink/nativeio/NativeAppUpdateReceiver;",
    b"Lorg/zerodenet/znetsink/nativeio/NativeAppUpdateReplacedReceiver;",
)
JNI_ENTRY_POINTS = (
    "Java_org_zerodenet_znetsink_nativeio_NativePluginJobs_nativeRun",
    "Java_org_zerodenet_znetsink_nativeio_NativePluginJobs_nativeStop",
)
NATIVE_PREFIX = "org.zerodenet.znetsink.nativeio."
JOB_SERVICES = (NATIVE_PREFIX + "NativePluginJobService", NATIVE_PREFIX + "NativeAppUpdateDownloadJob")
UPDATE_RECEIVERS = (NATIVE_PREFIX + "NativeAppUpdateReceiver", NATIVE_PREFIX + "NativeAppUpdateReplacedReceiver")
PERMISSION_PURPOSES = {
    "android.permission.ACCESS_NETWORK_STATE": "Required normal permission for network-constrained JobScheduler on target API34+; no system setting changes",
    "android.permission.INTERNET": "Existing proxy/plugin networking and bounded HTTPS update download",
    "android.permission.RECEIVE_BOOT_COMPLETED": "Persisted Android JobScheduler intents after reboot and existing notification restoration; no exact-time promise",
    "android.permission.REQUEST_INSTALL_PACKAGES": "Explicit user-reviewed app update through Android PackageInstaller; no unknown-source setting changes",
    "android.permission.WAKE_LOCK": "Existing notification plugin dependency; no unrestricted plugin wake lock",
    "android.permission.POST_NOTIFICATIONS": "Existing optional application notifications, separately subject to system permission",
}
MAX_APK = 1024 * 1024 * 1024
TARGETS = json.loads(pathlib.Path(__file__).with_name("android-build-targets.json").read_text())


def target_for_abi(expected_abi):
    matches = [target for target in TARGETS.values() if target["abi"] == expected_abi]
    if len(matches) != 1:
        raise ValueError("Unsupported expected Android ABI")
    return matches[0]


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def identity(path):
    current = path.lstat()
    if not stat.S_ISREG(current.st_mode) or current.st_size <= 0 or current.st_size > MAX_APK:
        raise ValueError("APK must be a bounded ordinary file")
    return (current.st_dev, current.st_ino, current.st_size,
            current.st_mtime_ns, current.st_ctime_ns)


def run_tool(arguments):
    try:
        result = subprocess.run([str(arg) for arg in arguments], check=False,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                timeout=120)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise ValueError("Android verification tool unavailable or timed out") from error
    if result.returncode != 0:
        raise ValueError("Android verification tool rejected the APK")
    if len(result.stdout) + len(result.stderr) > 2 * 1024 * 1024:
        raise ValueError("Android verification output exceeds budget")
    try:
        return result.stdout.decode("utf-8", errors="strict")
    except UnicodeError as error:
        raise ValueError("Android verification output is not UTF-8") from error


def verify_signature(output):
    # A zero exit alone is insufficient for a stub/incorrect tool invocation.
    if not re.search(r"^Verifies\s*$", output, re.M):
        raise ValueError("apksigner did not confirm verification")
    count = re.findall(r"^Number of signers:\s*(\d+)\s*$", output, re.M)
    certs = re.findall(r"^Signer #(\d+) certificate SHA-256 digest:\s*([0-9a-fA-F:]+)\s*$", output, re.M)
    if count != ["1"] or len(certs) != 1 or certs[0][0] != "1":
        raise ValueError("Exactly one verified APK signer is required")
    certificate = certs[0][1].replace(":", "").lower()
    if not re.fullmatch(r"[0-9a-f]{64}", certificate):
        raise ValueError("Invalid public signer certificate digest")
    schemes = re.findall(r"^Verified using (v[123][^:]*): (true|false)\s*$", output, re.M)
    if not schemes or not any(value == "true" for _, value in schemes):
        raise ValueError("No verified APK signature scheme")
    return {"verified": True, "minimumSdkVerified": 24,
            "signerCount": 1, "certificateSha256": certificate,
            "schemes": {name: value == "true" for name, value in schemes}}


def verify_badging(output, expected_abi="arm64-v8a"):
    target_for_abi(expected_abi)
    def exactly(pattern, expected, description):
        if re.findall(pattern, output, re.M) != [expected]:
            raise ValueError("Unexpected " + description)
    exactly(r"^package: name='([^']+)'[^\r\n]*$", APP_ID, "application ID")
    exactly(r"^sdkVersion:'([^']+)'\s*$", "24", "minimum API")
    exactly(r"^targetSdkVersion:'([^']+)'\s*$", "36", "target API")
    exactly(r"^launchable-activity: name='([^']+)'[^\r\n]*$", ACTIVITY, "launchable activity")
    native = re.findall(r"^native-code:\s*(.*?)\s*$", output, re.M)
    if native != [f"'{expected_abi}'"]:
        raise ValueError("Badging must declare only the expected ABI")
    return {"applicationId": APP_ID, "minimumSdk": 24, "targetSdk": 36,
            "launchableActivity": ACTIVITY, "abis": [expected_abi]}


def verify_debuggable(output):
    # Scope the typed boolean to <application>, never a child element or a string.
    application_depth = None
    applications = 0
    matches = []
    for line in output.splitlines():
        element = re.match(r"^(\s*)E: ([^\s(]+)(?:\s|$)", line)
        if element:
            depth = len(element[1])
            if application_depth is not None and depth <= application_depth:
                application_depth = None
            if element[2] == "application":
                applications += 1
                if applications != 1:
                    raise ValueError("Ambiguous application manifest")
                application_depth = depth
        if application_depth is not None and re.match(r"^\s*A: android:debuggable\(0x0101000f\)=", line):
            depth = len(line) - len(line.lstrip())
            if depth == application_depth + 2:
                matches.append(bool(re.search(r"=\(type 0x12\)0xffffffff\s*$", line)))
    if applications != 1 or matches != [True]:
        raise ValueError("Manifest application must explicitly be debuggable=true")
    return True


def verify_jni_exports(native):
    """Inspect defined ELF dynamic function symbols, not arbitrary matching strings."""
    if len(native) < 52 or native[:4] != b"\x7fELF" or native[5] != 1:
        raise ValueError("JNI export evidence requires a little-endian ELF")
    elf_class = native[4]
    if elf_class == 1:
        section_offset = struct.unpack_from("<I", native, 32)[0]
        section_bytes, section_count = struct.unpack_from("<HH", native, 46)
        section_format, symbol_format = "<IIIIIIIIII", "<IIIBBH"
    elif elf_class == 2 and len(native) >= 64:
        section_offset = struct.unpack_from("<Q", native, 40)[0]
        section_bytes, section_count = struct.unpack_from("<HH", native, 58)
        section_format, symbol_format = "<IIQQQQIIQQ", "<IBBHQQ"
    else:
        raise ValueError("Unsupported ELF class for JNI export evidence")
    if (not section_count or section_bytes != struct.calcsize(section_format)
            or section_offset < (64 if elf_class == 2 else 52) or section_offset + section_count * section_bytes > len(native)):
        raise ValueError("Invalid ELF section table for JNI export evidence")
    sections = [struct.unpack_from(section_format, native, section_offset + index * section_bytes)
                for index in range(section_count)]
    dynamic = [section for section in sections if section[1] == 11]  # SHT_DYNSYM
    if len(dynamic) != 1:
        raise ValueError("Exactly one ELF dynamic symbol table is required")
    table = dynamic[0]
    offset, size, link, entry_bytes = table[4], table[5], table[6], table[9]
    if (entry_bytes != struct.calcsize(symbol_format) or not size or size % entry_bytes
            or size // entry_bytes > 1_000_000 or offset + size > len(native) or link >= len(sections)):
        raise ValueError("Invalid ELF dynamic symbol bounds")
    strings = sections[link]
    if strings[1] != 3 or strings[5] > 16 * 1024 * 1024 or strings[4] + strings[5] > len(native):
        raise ValueError("Invalid ELF symbol string table")
    names = native[strings[4]:strings[4] + strings[5]]
    found = set()
    expected = {entry.encode("ascii") for entry in JNI_ENTRY_POINTS}
    for position in range(offset, offset + size, entry_bytes):
        symbol = struct.unpack_from(symbol_format, native, position)
        name_offset = symbol[0]
        info, visibility, section_index = (symbol[3], symbol[4], symbol[5]) if elf_class == 1 else (symbol[1], symbol[2], symbol[3])
        if name_offset >= len(names):
            raise ValueError("ELF dynamic symbol name is outside its string table")
        end = names.find(b"\0", name_offset, min(len(names), name_offset + 4096))
        if end < 0:
            raise ValueError("Unterminated ELF dynamic symbol name")
        name = bytes(names[name_offset:end])
        if name in expected:
            # GLOBAL DEFAULT FUNC, with an actual definition (SHN_UNDEF=0).
            value, function_size = (symbol[1], symbol[2]) if elf_class == 1 else (symbol[4], symbol[5])
            if info != 0x12 or visibility & 3 or not 0 < section_index < len(sections):
                raise ValueError("JNI entry point is not a defined public ELF function")
            implementation = sections[section_index]
            if (implementation[1] != 1 or not implementation[2] & 4 or not function_size
                    or implementation[4] + implementation[5] > len(native)
                    or not implementation[3] <= value < implementation[3] + implementation[5]
                    or value + function_size > implementation[3] + implementation[5]):
                raise ValueError("JNI function definition is outside its executable section")
            found.add(name.decode("ascii"))
    if found != set(JNI_ENTRY_POINTS):
        raise ValueError("Native library lacks headless JobScheduler JNI exports")
    return list(JNI_ENTRY_POINTS)


def dex_defined_entries(dex):
    """Follow class_def -> type_id -> string_id; a reference string is insufficient."""
    if (len(dex) < 112 or dex[:4] != b"dex\n" or bytes(dex[4:8]) not in
            {b"035\0", b"037\0", b"038\0", b"039\0", b"040\0"}):
        raise ValueError("Unsupported DEX header")
    if (struct.unpack_from("<III", dex, 32) != (len(dex), 112, 0x12345678)
            or hashlib.sha1(dex[32:]).digest() != dex[12:32]
            or zlib.adler32(dex[12:]) & 0xffffffff != struct.unpack_from("<I", dex, 8)[0]):
        raise ValueError("DEX header bounds or checksums are invalid")
    strings, string_offset, types, type_offset = struct.unpack_from("<IIII", dex, 56)
    classes, class_offset = struct.unpack_from("<II", dex, 96)
    for count, offset, width, maximum in ((strings, string_offset, 4, 1_000_000),
                                         (types, type_offset, 4, 65535), (classes, class_offset, 32, 65535)):
        if count > maximum or (count and (offset < 112 or offset + count * width > len(dex))):
            raise ValueError("DEX class/type/string table exceeds bounds")
    expected = set(ENTRY_POINTS)
    found, seen = set(), set()
    for index in range(classes):
        class_index = struct.unpack_from("<I", dex, class_offset + index * 32)[0]
        if class_index >= types or class_index in seen:
            raise ValueError("Invalid or duplicate DEX class definition")
        seen.add(class_index)
        string_index = struct.unpack_from("<I", dex, type_offset + class_index * 4)[0]
        if string_index >= strings:
            raise ValueError("DEX class descriptor index exceeds bounds")
        position = struct.unpack_from("<I", dex, string_offset + string_index * 4)[0]
        if position < 112 or position >= len(dex):
            raise ValueError("DEX descriptor data exceeds bounds")
        utf16_length = 0
        for shift in range(0, 35, 7):
            if position >= len(dex):
                raise ValueError("Truncated DEX string length")
            byte = dex[position]
            position += 1
            utf16_length |= (byte & 127) << shift
            if not byte & 128:
                break
        else:
            raise ValueError("Invalid DEX string length encoding")
        end = dex.find(b"\0", position, min(len(dex), position + 4096))
        if end < 0:
            raise ValueError("Unterminated or excessive DEX class descriptor")
        descriptor = bytes(dex[position:end])
        if descriptor in expected:
            if utf16_length != len(descriptor):
                raise ValueError("Required DEX class descriptor length mismatches")
            found.add(descriptor)
    return found


def manifest_nodes(output):
    roots, stack = [], []
    for line in output.splitlines():
        match = re.match(r"^(\s*)E: ([^\s(]+)(?:\s|$)", line)
        if match:
            depth = len(match[1])
            while stack and stack[-1][0] >= depth:
                stack.pop()
            node = {"tag": match[2], "attributes": {}, "children": []}
            if stack:
                if depth != stack[-1][0] + 2:
                    raise ValueError("Invalid manifest element nesting")
                stack[-1][1]["children"].append(node)
            else:
                roots.append(node)
            stack.append((depth, node))
            continue
        attribute = re.match(r"^(\s*)A: ([\w:.-]+)(?:\([^)]*\))?=(.*)$", line)
        if attribute:
            if not stack or len(attribute[1]) != stack[-1][0] + 2:
                raise ValueError("Invalid manifest attribute scope")
            attributes = stack[-1][1]["attributes"]
            if attribute[2] in attributes:
                raise ValueError("Duplicate manifest attribute")
            attributes[attribute[2]] = attribute[3]
    if len(roots) != 1 or roots[0]["tag"] != "manifest":
        raise ValueError("Exactly one manifest root is required")
    return roots[0]


def manifest_string(node, name, default=None):
    raw = node["attributes"].get(name)
    if raw is None:
        return default
    try:
        value, _ = json.JSONDecoder().raw_decode(raw)
    except ValueError as error:
        raise ValueError("Expected a literal manifest string: " + name) from error
    if not isinstance(value, str):
        raise ValueError("Expected a literal manifest string: " + name)
    return value


def manifest_boolean(node, name, expected, required=True):
    raw = node["attributes"].get(name)
    if raw is None and not required:
        return
    value = "0xffffffff" if expected else "0x0"
    if raw != "(type 0x12)" + value:
        raise ValueError("Unexpected typed manifest boolean: " + name)


def verify_platform_manifest(output, application_id=APP_ID):
    root = manifest_nodes(output)
    if manifest_string(root, "package") != application_id:
        raise ValueError("Manifest package identity mismatches expected application")
    applications = [node for node in root["children"] if node["tag"] == "application"]
    if len(applications) != 1:
        raise ValueError("Exactly one manifest application is required")
    application = applications[0]

    def same_process(node):
        if manifest_string(node, "android:process", application_id) != application_id:
            raise ValueError("Native jobs and application must share the same process")
        manifest_boolean(node, "android:isolatedProcess", False, required=False)
        manifest_boolean(node, "android:directBootAware", False, required=False)
        manifest_boolean(node, "android:enabled", True, required=False)

    same_process(application)
    permission_names = []
    for node in root["children"]:
        if node["tag"] in {"uses-permission", "uses-permission-sdk-23"}:
            name = manifest_string(node, "android:name")
            if name in permission_names:
                raise ValueError("Duplicate manifest permission")
            permission_names.append(name)
            if name in {"android.permission.INTERNET", "android.permission.ACCESS_NETWORK_STATE", "android.permission.RECEIVE_BOOT_COMPLETED", "android.permission.REQUEST_INSTALL_PACKAGES"} and "android:maxSdkVersion" in node["attributes"]:
                raise ValueError("Required platform permission cannot be SDK-limited")
    own_permission = application_id + ".DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION"
    allowed = set(PERMISSION_PURPOSES) | {own_permission}
    required = {"android.permission.INTERNET", "android.permission.ACCESS_NETWORK_STATE", "android.permission.RECEIVE_BOOT_COMPLETED", "android.permission.REQUEST_INSTALL_PACKAGES"}
    if not required.issubset(permission_names) or not set(permission_names).issubset(allowed):
        raise ValueError("Unexpected Android permission scope or missing job/installer permission")
    declared = [node for node in root["children"] if node["tag"] == "permission"]
    if declared:
        if (len(declared) != 1 or manifest_string(declared[0], "android:name") != own_permission
                or declared[0]["attributes"].get("android:protectionLevel") != "(type 0x11)0x2"):
            raise ValueError("Unexpected application-defined permission")
    elif own_permission in permission_names:
        raise ValueError("Private dynamic receiver permission must be declared with signature protection")

    def component(tag, name):
        matches = [node for node in application["children"] if node["tag"] == tag and manifest_string(node, "android:name") == name]
        if len(matches) != 1:
            raise ValueError("Missing or ambiguous native platform component: " + name)
        same_process(matches[0])
        return matches[0]

    component("activity", ACTIVITY)
    for name in JOB_SERVICES:
        service = component("service", name)
        if manifest_string(service, "android:permission") != "android.permission.BIND_JOB_SERVICE":
            raise ValueError("Job service must be guarded by BIND_JOB_SERVICE")
        manifest_boolean(service, "android:exported", True)
        if any(node["tag"] == "intent-filter" for node in service["children"]):
            raise ValueError("Job services must not expose an intent entry point")
    for name in UPDATE_RECEIVERS:
        receiver = component("receiver", name)
        manifest_boolean(receiver, "android:exported", False)
        actions = [manifest_string(action, "android:name") for group in receiver["children"] if group["tag"] == "intent-filter"
                   for action in group["children"] if action["tag"] == "action"]
        expected = ["android.intent.action.MY_PACKAGE_REPLACED"] if name.endswith("ReplacedReceiver") else []
        if actions != expected:
            raise ValueError("Unexpected update receiver intent scope")
    purposes = {name: PERMISSION_PURPOSES.get(name, "Existing AndroidX signature-private dynamic receiver dispatch") for name in permission_names}
    return {"jobServices": list(JOB_SERVICES), "privateUpdateReceivers": list(UPDATE_RECEIVERS),
            "sameProcess": application_id, "jobServicePermission": "android.permission.BIND_JOB_SERVICE",
            "permissionPurposes": purposes, "executionVerified": False}


def verify_contents(apk, expected_abi="arm64-v8a"):
    target = target_for_abi(expected_abi)
    host_library = f"lib/{expected_abi}/libgui_lib.so"
    with zipfile.ZipFile(apk) as archive:
        entries = archive.infolist()
        names = [entry.filename for entry in entries]
        if len(names) > 100000 or len(set(names)) != len(names):
            raise ValueError("Duplicate or excessive APK entries")
        for entry in entries:
            path = pathlib.PurePosixPath(entry.filename)
            if (path.is_absolute() or ".." in path.parts or "\\" in entry.filename
                    or stat.S_ISLNK(entry.external_attr >> 16)
                    or entry.flag_bits & 1 or entry.file_size > MAX_APK // 2):
                raise ValueError("Unsafe APK ZIP entry")
        if sum(entry.file_size for entry in entries) > MAX_APK:
            raise ValueError("Expanded APK exceeds budget")
        required = {"AndroidManifest.xml", "classes.dex", host_library, "assets/tauri.conf.json"}
        if not required.issubset(names) or archive.testzip() is not None:
            raise ValueError("Incomplete or corrupt APK ZIP")
        libraries = [name for name in names if name.startswith("lib/") and name.endswith(".so")]
        if not libraries or any(name.split("/")[1] != expected_abi for name in libraries):
            raise ValueError("APK must contain only the expected native ABI")
        for library in libraries:
            with archive.open(library) as handle:
                header = handle.read(64)
            if (len(header) < target["elfHeaderBytes"]
                    or header[:6] != b"\x7fELF" + bytes([target["elfClass"], 1])
                    or struct.unpack_from("<HH", header, 16) != (3, target["elfMachine"])):
                raise ValueError("Native payload ELF class or machine mismatches the expected ABI")
        dex_names = [name for name in names if re.fullmatch(r"classes(?:[2-9]|[1-9]\d+)?\.dex", name)]
        if sum(archive.getinfo(name).file_size for name in dex_names) > 256 * 1024 * 1024:
            raise ValueError("APK DEX contents exceed budget")
        defined = set().union(*(dex_defined_entries(archive.read(name)) for name in dex_names))
        if not set(ENTRY_POINTS).issubset(defined):
            raise ValueError("APK lacks required activity, document, job or installer entry points")
        native = archive.read(host_library)
        jni_exports = verify_jni_exports(native)
        try:
            config = json.loads(archive.read("assets/tauri.conf.json"))
        except (ValueError, UnicodeError) as error:
            raise ValueError("Invalid packaged Tauri configuration") from error
        if not isinstance(config, dict) or config.get("identifier") != TAURI_IDENTIFIER:
            raise ValueError("Packaged Tauri configuration has the wrong application ID")
        build = config.get("build")
        if not isinstance(build, dict) or build.get("devUrl") is not None:
            raise ValueError("Debug APK must bundle frontend without a development URL")
        # Locked tauri-codegen::EmbeddedAssets::to_tokens embeds these production
        # asset-map keys alongside include_bytes! payloads in this native library.
        # This is packaged-key evidence, not execution/rendering or source attestation.
        patterns = [rb"/_app/immutable/entry/app\.[A-Za-z0-9_-]+\.js",
                    rb"/_app/immutable/entry/start\.[A-Za-z0-9_-]+\.js",
                    rb"/_app/immutable/nodes/0\.[A-Za-z0-9_-]+\.js",
                    rb"/_app/immutable/nodes/2\.[A-Za-z0-9_-]+\.js"]
        keys = []
        if b"/index.html" not in native:
            raise ValueError("Bundled frontend index asset key is missing")
        for pattern in patterns:
            found = re.search(pattern, native)
            if not found:
                raise ValueError("Bundled production frontend asset keys are missing")
            keys.append(found[0].decode("ascii"))
        return {"abi": expected_abi, "targetTriple": target["triple"], "nativeLibrarySha256": hashlib.sha256(native).hexdigest(),
                "nativeDocumentsPackaged": True, "nativeBackgroundJobsPackaged": True, "nativeAppInstallerPackaged": True,
                "jniFunctionExports": jni_exports, "jniEvidence": "defined-public-ELF-dynamic-functions-only",
                "dexEvidence": "defined-class-identity-through-DEX-class-type-string-tables",
                "nativeEntryPoints": [entry.decode("ascii") for entry in ENTRY_POINTS],
                "frontend": {"storage": "tauri-native-embedded-assets", "index": "/index.html",
                             "observedEmbeddedAssetKeys": keys, "evidence": "packaged-asset-keys-only",
                             "developmentUrlAbsent": True,
                             "executionVerified": False}, "zipIntegrity": "all-entry-crc-verified"}


def verify_debug(apk, build_tools, expected_commit, receipt, expected_abi="arm64-v8a"):
    if not re.fullmatch(r"[0-9a-f]{40}", expected_commit):
        raise ValueError("Expected commit must be a full lowercase Git SHA")
    if receipt.resolve() == apk.resolve():
        raise ValueError("Receipt cannot overwrite the APK")
    before = identity(apk)
    apk_sha = digest(apk)
    contents = verify_contents(apk, expected_abi)
    signature = verify_signature(run_tool([build_tools / "apksigner", "verify", "--verbose", "--print-certs", "--min-sdk-version", "24", apk]))
    badging = run_tool([build_tools / "aapt", "dump", "badging", apk])
    package = verify_badging(badging, expected_abi)
    manifest = run_tool([build_tools / "aapt", "dump", "xmltree", apk, "AndroidManifest.xml"])
    verify_debuggable(manifest)
    platform_manifest = verify_platform_manifest(manifest)
    if identity(apk) != before or digest(apk) != apk_sha:
        raise ValueError("APK changed during verification")
    result = {"schemaVersion": "znet.debug-android-apk.v1", "apk": apk.name,
              "sha256": apk_sha, "bytes": before[2], "commit": expected_commit,
              "buildType": "debug", "applicationId": APP_ID, "activity": ACTIVITY,
              "androidApplicationId": APP_ID, "tauriIdentifier": TAURI_IDENTIFIER,
              "namespace": NAMESPACE, "debuggable": True, "signature": signature, "package": package,
              "platformManifest": platform_manifest,
              "packageProof": {"badgingSha256": hashlib.sha256(badging.encode()).hexdigest(),
                               "manifestXmltreeSha256": hashlib.sha256(manifest.encode()).hexdigest()},
              **contents}
    # No receipt is published on failure. Atomic replacement also prevents partial JSON.
    receipt.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=receipt.parent,
                                         prefix=".debug-apk-receipt-", delete=False) as handle:
            temporary = pathlib.Path(handle.name)
            json.dump(result, handle, ensure_ascii=False, indent=2)
            handle.write("\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, receipt)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("apk", type=pathlib.Path)
    parser.add_argument("--build-tools", type=pathlib.Path, required=True)
    parser.add_argument("--expected-commit", required=True)
    parser.add_argument("--expected-abi", choices=[target["abi"] for target in TARGETS.values()], default="arm64-v8a")
    parser.add_argument("--receipt", type=pathlib.Path, required=True)
    args = parser.parse_args()
    try:
        result = verify_debug(args.apk, args.build_tools, args.expected_commit, args.receipt, args.expected_abi)
    except (ValueError, OSError, zipfile.BadZipFile, RuntimeError) as error:
        print(str(error), file=sys.stderr)
        return 1
    print(json.dumps(result, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
