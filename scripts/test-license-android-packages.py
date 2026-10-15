#!/usr/bin/env python3
"""Offline tests of the production Android publisher-material collector."""
import importlib.util
import io
import sys
import tempfile
import sys
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

sys.dont_write_bytecode = True

sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location("android_materials", Path(__file__).with_name("license-android-packages.py"))
collector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(collector)


def archive(entries):
    output = io.BytesIO()
    with zipfile.ZipFile(output, "w") as bundle:
        for name, content in entries.items():
            bundle.writestr(name, content)
    return output.getvalue()


class AndroidMaterialTests(unittest.TestCase):
    def test_source_header_bytes_are_not_a_full_license(self):
        content = b'/* Copyright 2024 Fixture author\n * Licensed under Apache 2.0\n */\npackage fixture;\n'
        data = archive({"Main.java": content, "LICENSE.spdx": "SPDXVersion: SPDX-2.1"})
        materials, gaps, count = collector.archive_materials(data, "https://example.invalid/pinned.jar", "maven:fixture:sample:1", sources=True)
        self.assertEqual(count, 1)
        self.assertFalse(gaps)
        self.assertTrue(all(material["kind"] == "notice" for material in materials))
        header = next(material for material in materials if material["name"].startswith("Main.java"))
        span = header["origin"]["selection"]
        self.assertEqual(header["text"].encode(), content[span["startByte"]:span["endByte"]])
        self.assertNotIn("package fixture", header["text"])

    def test_identical_notices_deduplicate_but_keep_origin_occurrences(self):
        data = archive({"One.java": "// Copyright 2024 Fixture\npackage a;", "Two.kt": "// Copyright 2024 Fixture\npackage b"})
        materials, _, _ = collector.archive_materials(data, "https://example.invalid/pinned.jar", "maven:fixture:sample:1", sources=True)
        self.assertEqual(len(materials), 1)
        self.assertEqual(len(materials[0]["occurrences"]), 2)

    def test_code_strings_are_never_invented_as_notice(self):
        self.assertIsNone(collector.attribution_prefix(b'package a; String value = "Copyright 2024";'))
        self.assertIsNone(collector.attribution_prefix(b'/** Ordinary documentation */\npackage a;'))
        self.assertIsNone(collector.attribution_prefix(b'/* Copyright unfinished'))

    def test_utf8_failure_remains_explicit(self):
        materials, gaps, _ = collector.archive_materials(archive({"LICENSE": b"\xff"}), "https://example.invalid/x", "fixture")
        self.assertEqual(materials, [])
        self.assertEqual(gaps, ["publisher-material-not-utf8"])

    def test_source_module_scope_cannot_copy_sibling_copyright(self):
        data = archive({"module/Main.java": "// Copyright 2024 Real owner\npackage a;", "sibling/LICENSE": "Unrelated license"})
        materials, _, count = collector.archive_materials(data, "https://example.invalid/x", "fixture", sources=True, member_prefix="module/")
        self.assertEqual(count, 1)
        self.assertEqual(len(materials), 1)
        self.assertNotIn("Unrelated", materials[0]["text"])

    def test_unsafe_or_over_budget_archive_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "unsafe-source"):
            collector.archive_materials(archive({"../LICENSE": "unsafe"}), "https://example.invalid/x", "fixture")
        with patch.object(collector, "MAX_TOTAL_EXPANDED", 2):
            with self.assertRaisesRegex(ValueError, "over-budget"):
                collector.archive_materials(archive({"LICENSE": "large"}), "https://example.invalid/x", "fixture")

    def fixture(self, artifact_name="sample-1.jar"):
        row = {"id": "maven:fixture:sample:1", "ecosystem": "maven", "name": "sample", "version": "1", "source": "maven:fixture:sample:1"}
        jar, pom = b"actual selected artifact", b"actual selected POM"
        inputs = [{"label": row["id"] + "!" + artifact_name, "sha256": collector.digest(jar)}, {"label": row["id"] + "!pom", "sha256": collector.digest(pom)}]
        source = archive({"Main.java": "// Copyright 2024 Actual fixture author\npackage a;"})
        return row, inputs, jar, pom, source

    def test_selected_artifact_digest_mismatch_never_yields_material(self):
        row, inputs, *_ = self.fixture()
        with patch.object(collector, "download", return_value=b"different artifact"):
            result = collector.collect_package(row, inputs, Path("unused"))
        self.assertEqual(result["materials"], [])
        self.assertEqual(result["resolves"], [])
        self.assertIn("selected-publisher-artifact-digest-mismatch", result["gaps"])

    def test_pom_digest_mismatch_also_rejects(self):
        row, inputs, jar, _, _ = self.fixture()
        with patch.object(collector, "download", side_effect=lambda url, _: b"different POM" if url.endswith(".pom") else jar):
            result = collector.collect_package(row, inputs, Path("unused"))
        self.assertFalse(result["materials"])
        self.assertIn("selected-publisher-artifact-digest-mismatch", result["gaps"])

    def test_gradle_renamed_variant_still_verifies_canonical_publisher_bytes(self):
        row, inputs, jar, pom, sources = self.fixture("sample.jar")
        seen = []
        def fetch(url, _):
            seen.append(url)
            return sources if url.endswith("-sources.jar") else pom if url.endswith(".pom") else jar
        with patch.object(collector, "download", side_effect=fetch):
            first = collector.collect_package(row, inputs, Path("unused"))
            second = collector.collect_package(row, inputs, Path("unused"))
        self.assertEqual(first, second)
        self.assertTrue(any(url.endswith("sample-1.jar") for url in seen))
        self.assertEqual(first["identity"], {field: row[field] for field in ("ecosystem", "name", "version", "source")})
        self.assertEqual(first["expected"]["artifactSha256"], collector.digest(jar))
        self.assertEqual(first["expected"]["pomSha256"], collector.digest(pom))
        self.assertEqual(first["resolves"], ["package-specific-legal-material-not-provided"])
        material = first["materials"][0]
        self.assertEqual(material["origin"]["sha256"], collector.digest(sources))
        self.assertEqual(collector.digest(material["text"].encode()), material["sha256"])
        self.assertIn("byteRange", material["origin"])
        self.assertEqual(material["kind"], "notice")
        self.assertIn("publisher-source-full-license-file-not-present", first["gaps"])

    def test_duplicate_views_do_not_hash_the_same_selected_archive_twice(self):
        row, inputs, jar, pom, sources = self.fixture()
        def fetch(url, _):
            return sources if url.endswith("-sources.jar") else pom if url.endswith(".pom") else jar
        with patch.object(collector, "download", side_effect=fetch):
            first = collector.collect_package(row, inputs, Path("unused"))
            duplicated = collector.collect_package(row, inputs + [dict(inputs[0])], Path("unused"))
        self.assertEqual(first, duplicated)
        self.assertEqual(duplicated["expected"]["artifactSha256"], collector.digest(jar))

    def test_no_pom_binding_or_arbitrary_origin_is_accepted(self):
        row, inputs, *_ = self.fixture()
        result = collector.collect_package(row, inputs[:1], Path("unused"))
        self.assertFalse(result["materials"])
        self.assertIn("selected-artifact-or-pom-binding-unavailable", result["gaps"])
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(ValueError, "unapproved"):
                collector.download("https://untrusted.invalid/license", Path(directory))

    def test_unversioned_or_path_like_gav_is_rejected(self):
        row, _, *_ = self.fixture()
        row["id"] = "maven:fixture:sample:../../latest"
        with self.assertRaisesRegex(ValueError, "identity"):
            collector.collect_package(row, [], Path("unused"))


if __name__ == "__main__":
    unittest.main()
