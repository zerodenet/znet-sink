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
)
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
        dex = b"".join(archive.read(name) for name in dex_names)
        if not all(entry in dex for entry in ENTRY_POINTS):
            raise ValueError("APK lacks MainActivity or native document entry points")
        native = archive.read(host_library)
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
                "nativeDocumentsPackaged": True, "nativeEntryPoints": [entry.decode("ascii") for entry in ENTRY_POINTS],
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
    if identity(apk) != before or digest(apk) != apk_sha:
        raise ValueError("APK changed during verification")
    result = {"schemaVersion": "znet.debug-android-apk.v1", "apk": apk.name,
              "sha256": apk_sha, "bytes": before[2], "commit": expected_commit,
              "buildType": "debug", "applicationId": APP_ID, "activity": ACTIVITY,
              "androidApplicationId": APP_ID, "tauriIdentifier": TAURI_IDENTIFIER,
              "namespace": NAMESPACE, "debuggable": True, "signature": signature, "package": package,
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
