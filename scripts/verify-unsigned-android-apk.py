#!/usr/bin/env python3
"""Inspect a build-owned APK without executing or signing it.

Uses existing aapt to inspect packaged job/installer declarations. This is
package evidence, never a claim of OS execution or installation acceptance.
"""
import argparse
import hashlib
import importlib.util
import os
import pathlib
import shutil
import struct
import sys
import zipfile

SPEC = importlib.util.spec_from_file_location("android_package_verifier", pathlib.Path(__file__).with_name("verify-debug-android-apk.py"))
PACKAGE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PACKAGE)


def resolve_build_tools(directory=None):
    if directory is not None:
        return pathlib.Path(directory)
    sdk = os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT")
    if sdk:
        for version in ("35.0.0", "36.0.0"):
            tools = pathlib.Path(sdk) / "build-tools" / version
            if (tools / "aapt").is_file():
                return tools
    aapt = shutil.which("aapt")
    if aapt:
        return pathlib.Path(aapt).parent
    raise ValueError("Existing Android aapt is required; pass --build-tools or set ANDROID_HOME")


def verify_unsigned(apk: pathlib.Path, build_tools=None) -> dict:
    before = PACKAGE.identity(apk)
    apk_sha = PACKAGE.digest(apk)
    contents = PACKAGE.verify_contents(apk, "arm64-v8a")
    with zipfile.ZipFile(apk) as archive:
        names = archive.namelist()
        signatures = [name for name in names if name.upper().startswith("META-INF/")
                      and name.upper().endswith((".RSA", ".DSA", ".EC", ".SF"))]
        if signatures:
            raise ValueError("APK contains a v1 signature entry")
    with apk.open("rb") as handle:
        handle.seek(0, 2)
        size = handle.tell()
        handle.seek(max(0, size - 65557))
        tail = handle.read()
        eocd = tail.rfind(b"PK\x05\x06")
        if eocd < 0 or eocd + 22 > len(tail):
            raise ValueError("missing ZIP end record")
        comment_length = struct.unpack_from("<H", tail, eocd + 20)[0]
        if eocd + 22 + comment_length != len(tail):
            raise ValueError("invalid ZIP end record")
        central_offset = struct.unpack_from("<I", tail, eocd + 16)[0]
        if central_offset == 0xFFFFFFFF:
            raise ValueError("ZIP64 APK is outside this verification contract")
        if central_offset < 24 or central_offset > size:
            raise ValueError("invalid APK central directory")
        handle.seek(central_offset - 16)
        if handle.read(16) == b"APK Sig Block 42":
            raise ValueError("APK contains a v2/v3 signing block")
    if apk.with_suffix(apk.suffix + ".idsig").exists():
        raise ValueError("APK has an adjacent v4 signature sidecar")
    tools = resolve_build_tools(build_tools)
    manifest = PACKAGE.run_tool([tools / "aapt", "dump", "xmltree", apk, "AndroidManifest.xml"])
    platform_manifest = PACKAGE.verify_platform_manifest(manifest, PACKAGE.TAURI_IDENTIFIER)
    application = next(node for node in PACKAGE.manifest_nodes(manifest)["children"] if node["tag"] == "application")
    PACKAGE.manifest_boolean(application, "android:debuggable", False, required=False)
    if PACKAGE.identity(apk) != before or PACKAGE.digest(apk) != apk_sha:
        raise ValueError("APK changed during verification")
    return {"apk": apk.name, "bytes": size, "sha256": apk_sha, "unsigned": True,
            "platformManifest": platform_manifest,
            "manifestXmltreeSha256": hashlib.sha256(manifest.encode()).hexdigest(), **contents}


if __name__ == "__main__":
    import json
    try:
        parser = argparse.ArgumentParser(description=__doc__)
        parser.add_argument("apk", type=pathlib.Path)
        parser.add_argument("--build-tools", type=pathlib.Path)
        arguments = parser.parse_args()
        print(json.dumps(verify_unsigned(arguments.apk, arguments.build_tools)))
    except (ValueError, OSError, zipfile.BadZipFile, RuntimeError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
