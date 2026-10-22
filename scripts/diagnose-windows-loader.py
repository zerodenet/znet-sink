#!/usr/bin/env python3
"""Bounded, read-only PE import diagnostics; never executes a test EXE.

DLL probes run in this Python process, whose preloaded DLLs/search context may
vary from Cargo's child. Loaded paths are evidence for this probe, not a claim
that the failed test selected the same path. No DLLs or machine settings change.
"""
import argparse
import ctypes
from collections import deque
import json
import mmap
import os
from pathlib import Path
import struct
import sys


class PEError(ValueError):
    pass


def pe_imports(data, max_symbols=4096):
    """Parse file-backed PE32/PE32+ normal and delay imports, with hard bounds."""
    def unpack(fmt, offset):
        size = struct.calcsize(fmt)
        if offset < 0 or offset + size > len(data):
            raise PEError("truncated PE field")
        return struct.unpack_from(fmt, data, offset)

    if len(data) < 64 or data[:2] != b"MZ":
        raise PEError("missing DOS header")
    pe = unpack("<I", 60)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise PEError("missing PE signature")
    sections = unpack("<H", pe + 6)[0]
    optional_size = unpack("<H", pe + 20)[0]
    optional = pe + 24
    if optional_size < 96 or optional + optional_size > len(data) or sections > 96:
        raise PEError("invalid PE headers")
    magic = unpack("<H", optional)[0]
    if magic == 0x10B:
        width, directory, count_offset = 4, 96, 92
        image_base = unpack("<I", optional + 28)[0]
    elif magic == 0x20B:
        width, directory, count_offset = 8, 112, 108
        image_base = unpack("<Q", optional + 24)[0]
    else:
        raise PEError("unsupported optional header")
    if optional_size < directory:
        raise PEError("truncated optional header directories")
    count = unpack("<I", optional + count_offset)[0]
    headers_size = unpack("<I", optional + 60)[0]
    mappings = []
    for index in range(sections):
        start = optional + optional_size + index * 40
        virtual_size, virtual, raw_size, raw = unpack("<IIII", start + 8)
        mappings.append((virtual, max(virtual_size, raw_size), raw, raw_size))

    def offset(rva, length=1):
        if rva < headers_size and rva + length <= min(headers_size, len(data)):
            return rva
        for virtual, size, raw, raw_size in mappings:
            delta = rva - virtual
            if 0 <= delta < size:
                if delta + length > raw_size or raw + delta + length > len(data):
                    raise PEError("RVA has no complete file-backed data")
                return raw + delta
        raise PEError("RVA outside PE sections")

    def string(rva):
        result = bytearray()
        for index in range(4096):
            byte = data[offset(rva + index)]
            if byte == 0:
                try:
                    return result.decode("ascii")
                except UnicodeDecodeError as error:
                    raise PEError("non-ASCII import") from error
            result.append(byte)
        raise PEError("unterminated import string")

    result, total = [], 0
    for directory_index, delayed in [(1, False), (13, True)]:
        if count <= directory_index:
            continue
        relative = directory + directory_index * 8
        if relative + 8 > optional_size:
            raise PEError("data directory exceeds optional header")
        rva, size = unpack("<II", optional + relative)
        if not rva:
            continue
        descriptor_size = 32 if delayed else 20
        terminated = False
        for index in range(min(size // descriptor_size, 256)):
            fields = unpack("<8I" if delayed else "<5I", offset(rva + index * descriptor_size, descriptor_size))
            if not any(fields):
                terminated = True
                break
            if delayed:
                attrs, name, _, iat, names, _, _, _ = fields
                convert = (lambda value: value) if attrs & 1 else (lambda value: value - image_base)
                name, thunk = convert(name), convert(names or iat)
            else:
                names, _, _, name, iat = fields
                thunk = names or iat
            symbols = []
            for item in range(max_symbols + 1):
                value = unpack("<Q" if width == 8 else "<I", offset(thunk + item * width, width))[0]
                if not value:
                    break
                total += 1
                if total > max_symbols:
                    raise PEError("import symbol budget exceeded")
                if value & (1 << (width * 8 - 1)):
                    symbols.append({"ordinal": value & 0xFFFF})
                else:
                    symbols.append({"name": string(value + 2)})
            result.append({"dll": string(name), "delayed": delayed, "symbols": symbols})
        if not terminated:
            raise PEError("unterminated or over-budget import descriptors")
    return result


def imports_from_file(path):
    with path.open("rb") as source:
        if os.fstat(source.fileno()).st_size == 0:
            raise PEError("empty file")
        with mmap.mmap(source.fileno(), 0, access=mmap.ACCESS_READ) as data:
            return pe_imports(data)


def emit(event, **fields):
    print(json.dumps({"event": event, **fields}, ensure_ascii=True), flush=True)


class NativeProbe:
    def __init__(self, directories):
        self.kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        self.kernel.LoadLibraryExW.argtypes = [ctypes.c_wchar_p, ctypes.c_void_p, ctypes.c_uint32]
        self.kernel.LoadLibraryExW.restype = ctypes.c_void_p
        self.kernel.GetModuleFileNameW.argtypes = [ctypes.c_void_p, ctypes.c_wchar_p, ctypes.c_uint32]
        self.kernel.GetProcAddress.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
        self.kernel.GetProcAddress.restype = ctypes.c_void_p
        self.kernel.FreeLibrary.argtypes = [ctypes.c_void_p]
        self.kernel.AddDllDirectory.argtypes = [ctypes.c_wchar_p]
        self.kernel.AddDllDirectory.restype = ctypes.c_void_p
        self.kernel.RemoveDllDirectory.argtypes = [ctypes.c_void_p]
        self.kernel.SetErrorMode(0x0001 | 0x0002 | 0x8000)
        self.cookies = [self.kernel.AddDllDirectory(str(path)) for path in directories]
        self.version = ctypes.WinDLL("version", winmode=0x800, use_last_error=True)
        self.version.GetFileVersionInfoSizeW.argtypes = [ctypes.c_wchar_p, ctypes.c_void_p]
        self.version.GetFileVersionInfoSizeW.restype = ctypes.c_uint32
        self.version.GetFileVersionInfoW.argtypes = [ctypes.c_wchar_p, ctypes.c_uint32, ctypes.c_uint32, ctypes.c_void_p]
        self.version.VerQueryValueW.argtypes = [ctypes.c_void_p, ctypes.c_wchar_p, ctypes.POINTER(ctypes.c_void_p), ctypes.POINTER(ctypes.c_uint32)]
        self.handles = []

    def file_version(self, path):
        size = self.version.GetFileVersionInfoSizeW(str(path), None)
        if not size or size > 16 * 1024 * 1024:
            return None
        buffer = ctypes.create_string_buffer(size)
        if not self.version.GetFileVersionInfoW(str(path), 0, size, buffer):
            return None
        pointer, length = ctypes.c_void_p(), ctypes.c_uint32()
        if not self.version.VerQueryValueW(buffer, "\\", ctypes.byref(pointer), ctypes.byref(length)) or length.value < 52:
            return None
        values = struct.unpack("<13I", ctypes.string_at(pointer, 52))
        return ".".join(str(part) for part in [values[2] >> 16, values[2] & 65535, values[3] >> 16, values[3] & 65535])

    def load(self, request):
        # Search only explicit build directories and System32, not arbitrary PATH.
        flags = 0x800 | 0x400 | (0x100 if Path(request).is_absolute() else 0)
        handle = self.kernel.LoadLibraryExW(str(request), None, flags)
        if not handle:
            return None, None, ctypes.get_last_error()
        self.handles.append(handle)
        name = ctypes.create_unicode_buffer(32768)
        if not self.kernel.GetModuleFileNameW(handle, name, len(name)):
            return handle, None, ctypes.get_last_error()
        return handle, Path(name.value), None

    def missing(self, handle, symbols):
        missing = []
        for symbol in symbols:
            argument = ctypes.c_void_p(symbol["ordinal"]) if "ordinal" in symbol else ctypes.cast(ctypes.c_char_p(symbol["name"].encode("ascii")), ctypes.c_void_p)
            if not self.kernel.GetProcAddress(handle, argument):
                missing.append(symbol)
        return missing

    def close(self):
        for handle in reversed(self.handles):
            self.kernel.FreeLibrary(handle)
        for cookie in self.cookies:
            if cookie:
                self.kernel.RemoveDllDirectory(cookie)


def diagnose(executable, max_modules):
    root = executable.parent.parent
    directories = [executable.parent, root]
    # Cargo's native build outputs only; do not crawl user/PATH directories.
    build = root / "build"
    if build.is_dir():
        for dll in build.rglob("*.dll"):
            directory = dll.parent.resolve()
            if directory not in directories:
                directories.append(directory)
            if len(directories) >= 128:
                emit("directory_budget", limit=128)
                break
    emit("context", executable=str(executable), directories=[str(path) for path in directories],
         limitation="Probe DLL paths may differ from the failed Cargo child; no test EXE is executed. API-set names may map to another DLL.")
    probe = NativeProbe(directories)
    queue, visited, libraries = deque([executable]), set(), {}
    module_budget_reported = False
    try:
        while queue and len(visited) < max_modules:
            importer = queue.popleft()
            key = os.path.normcase(str(importer))
            if key in visited:
                continue
            visited.add(key)
            try:
                imports = imports_from_file(importer)
            except (PEError, OSError, ValueError) as error:
                emit("parse_error", file=str(importer), reason=str(error))
                continue
            for entry in imports:
                dll = entry["dll"]
                if Path(dll).name != dll or not dll.lower().endswith(".dll"):
                    emit("invalid_import_name", importer=str(importer), name=dll)
                    continue
                if dll.lower() not in libraries:
                    if len(libraries) >= max_modules:
                        if not module_budget_reported:
                            emit("module_budget", limit=max_modules)
                            module_budget_reported = True
                        continue
                    candidates = [path / dll for path in directories if (path / dll).is_file()]
                    system_file = Path(os.environ.get("SystemRoot", "C:/Windows")) / "System32" / dll
                    system_candidate = system_file if system_file.is_file() else None
                    request = str(candidates[0]) if candidates else dll
                    handle, actual, error = probe.load(request)
                    libraries[dll.lower()] = (handle, actual, error)
                    emit("dll", requested=dll, candidates=[str(path) for path in candidates],
                         loaded_path=str(actual) if actual else None, system_file=str(system_candidate) if system_candidate else None,
                         version=probe.file_version(actual) if actual else None,
                         win32_load_error=error, api_set=dll.lower().startswith(("api-", "ext-")))
                    if actual:
                        queue.append(actual)
                    elif candidates or system_candidate:
                        # Even a failed OS DLL may import the missing symbol.
                        # Parse its file without claiming it loaded successfully.
                        queue.append(candidates[0] if candidates else system_candidate)
                handle, actual, error = libraries[dll.lower()]
                emit("imports", importer=str(importer), dll=dll, delayed=entry["delayed"], count=len(entry["symbols"]),
                     symbols=entry["symbols"] if importer == executable else None,
                     missing_symbols=probe.missing(handle, entry["symbols"]) if handle else None,
                     win32_load_error=error)
        if queue:
            emit("scan_budget", visited=len(visited), pending=len(queue), limit=max_modules)
    finally:
        probe.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("path", type=Path, help="gui_lib test EXE or its debug/deps directory")
    parser.add_argument("--max-modules", type=int, default=64)
    args = parser.parse_args()
    if not 1 <= args.max_modules <= 256:
        parser.error("--max-modules must be 1..256")
    path = args.path.resolve()
    binaries = sorted(path.glob("gui_lib*.exe")) if path.is_dir() else [path]
    if not binaries:
        emit("no_test_executable", path=str(path))
        return 1
    if sys.platform != "win32":
        emit("unsupported_host", platform=sys.platform, reason="Native DLL probes require Windows")
        return 1
    for executable in binaries[:4]:
        diagnose(executable, args.max_modules)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
