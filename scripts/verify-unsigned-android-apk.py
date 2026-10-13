#!/usr/bin/env python3
"""Inspect a build-owned APK without executing or signing it."""
import pathlib
import struct
import sys
import zipfile


def verify_unsigned(apk: pathlib.Path) -> dict:
    with zipfile.ZipFile(apk) as archive:
        names = archive.namelist()
        required = {"AndroidManifest.xml", "classes.dex", "lib/arm64-v8a/libgui_lib.so"}
        if not required.issubset(names):
            raise ValueError("APK lacks the arm64 application host")
        dex = b"".join(archive.read(name) for name in names
                       if name.startswith("classes") and name.endswith(".dex"))
        entry_points = (b"Lorg/zerodenet/znetsink/MainActivity;",
                        b"Lorg/zerodenet/znetsink/nativeio/NativeIoPlugin;",
                        b"Lorg/zerodenet/znetsink/nativeio/NativeIoFileProvider;",
                        b"Lorg/zerodenet/znetsink/nativeio/ShareSelectionReceiver;")
        if not all(entry in dex for entry in entry_points):
            raise ValueError("APK lacks native document entry points")
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
    return {"apk": apk.name, "bytes": size, "unsigned": True, "abi": "arm64-v8a", "nativeDocumentsPackaged": True}


if __name__ == "__main__":
    import json
    try:
        if len(sys.argv) != 2:
            raise ValueError("usage: verify-unsigned-android-apk.py APK")
        print(json.dumps(verify_unsigned(pathlib.Path(sys.argv[1]))))
    except (ValueError, OSError, zipfile.BadZipFile) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
