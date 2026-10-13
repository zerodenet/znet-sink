#!/usr/bin/env python3
"""PE import parser regression, portable and independent of Windows execution."""
import importlib.util
from pathlib import Path
import struct
import unittest

spec = importlib.util.spec_from_file_location("loader_diagnostic", Path(__file__).with_name("diagnose-windows-loader.py"))
loader = importlib.util.module_from_spec(spec)
spec.loader.exec_module(loader)


def fixture(bits=64, original_thunk=True):
    data = bytearray(0xA00)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 60, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    optional, optional_size = 0x98, 240 if bits == 64 else 224
    struct.pack_into("<HH", data, 0x84, 0x8664 if bits == 64 else 0x14C, 1)
    struct.pack_into("<H", data, 0x94, optional_size)
    struct.pack_into("<H", data, optional, 0x20B if bits == 64 else 0x10B)
    struct.pack_into("<I", data, optional + 60, 0x200)
    directory = optional + (112 if bits == 64 else 96)
    struct.pack_into("<I", data, directory - 4, 16)
    struct.pack_into("<Q" if bits == 64 else "<I", data, optional + (24 if bits == 64 else 28), 0x400000)
    struct.pack_into("<II", data, directory + 8, 0x1000, 40)
    struct.pack_into("<II", data, directory + 13 * 8, 0x1100, 64)
    section = optional + optional_size
    data[section:section + 8] = b".imports"
    struct.pack_into("<IIII", data, section + 8, 0x900, 0x1000, 0x800, 0x200)
    struct.pack_into("<5I", data, 0x200, 0x1040 if original_thunk else 0, 0, 0, 0x1080, 0x1060)
    thunk = "<3Q" if bits == 64 else "<3I"
    ordinal = 1 << (bits - 1)
    for offset in [0x240, 0x260]:
        struct.pack_into(thunk, data, offset, 0x10C0, ordinal | 7, 0)
    data[0x280:0x280 + 13] = b"kernel32.dll\0"
    data[0x2C0:0x2C0 + 8] = b"\0\0Named\0"
    struct.pack_into("<8I", data, 0x300, 1, 0x1180, 0, 0x11C0, 0x11A0, 0, 0, 0)
    data[0x380:0x380 + 14] = b"delayload.dll\0"
    struct.pack_into("<2Q" if bits == 64 else "<2I", data, 0x3A0, 0x11E0, 0)
    data[0x3E0:0x3E0 + 10] = b"\0\0Delayed\0"
    return data


class Imports(unittest.TestCase):
    def test_pe32_and_pe64_names_ordinals_and_delay_imports(self):
        for bits in [32, 64]:
            with self.subTest(bits=bits):
                parsed = loader.pe_imports(fixture(bits))
                self.assertEqual(parsed[0], {"dll": "kernel32.dll", "delayed": False,
                                            "symbols": [{"name": "Named"}, {"ordinal": 7}]})
                self.assertEqual(parsed[1], {"dll": "delayload.dll", "delayed": True,
                                            "symbols": [{"name": "Delayed"}]})

    def test_missing_original_thunk_uses_file_iat(self):
        self.assertEqual(loader.pe_imports(fixture(original_thunk=False))[0]["symbols"],
                         [{"name": "Named"}, {"ordinal": 7}])

    def test_unbacked_virtual_rva_is_rejected(self):
        data = fixture()
        struct.pack_into("<Q", data, 0x240, 0x1880)
        with self.assertRaisesRegex(loader.PEError, "file-backed"):
            loader.pe_imports(data)

    def test_descriptor_termination_is_required(self):
        data = fixture()
        struct.pack_into("<I", data, 0x98 + 112 + 8 + 4, 20)
        with self.assertRaisesRegex(loader.PEError, "unterminated"):
            loader.pe_imports(data)

    def test_symbol_budget_prevents_unbounded_tables(self):
        with self.assertRaisesRegex(loader.PEError, "budget"):
            loader.pe_imports(fixture(), max_symbols=1)

    def test_truncated_and_non_pe_input_is_rejected(self):
        for data in [b"", b"MZ", fixture()[:0x150]]:
            with self.subTest(length=len(data)), self.assertRaises(loader.PEError):
                loader.pe_imports(data)


if __name__ == "__main__":
    unittest.main()
