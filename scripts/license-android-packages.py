#!/usr/bin/env python3
"""Explicit maintenance collector for exact selected Maven publisher materials.

Never used by ordinary builds, never resolves a newer package version. Inputs
come from the actual Gradle release runtime inventory; source archives and POMs
are fetched only from official Maven publisher repositories. Source headers are
verbatim attribution notices, not inferred full license texts.
"""
import argparse
import concurrent.futures
import hashlib
import io
import json
import re
import urllib.error
import urllib.request
import zipfile
from pathlib import Path

MAX_ARCHIVE = 64 * 1024 * 1024
MAX_TEXT = 1024 * 1024
MAX_TOTAL_EXPANDED = 128 * 1024 * 1024
COROUTINES_REVISION = "d8d6f8f37978b8e202d93b34f23f101df9c5724d"
CONSTRAINT_REVISION = "272719edf76066c3bee82987012da9349c760233"
# Reviewed exact official release tags, not mutable branches or latest-version
# material. Coroutines POM names this source repo; constraintlayout's release
# source retains the same sherpa SCM publisher declaration as its published POM.
SUPPLEMENTAL_PREFIXES = (
    "https://raw.githubusercontent.com/Kotlin/kotlinx.coroutines/" + COROUTINES_REVISION + "/",
    "https://codeload.github.com/androidx/constraintlayout/zip/" + CONSTRAINT_REVISION,
)
COORDINATE = re.compile(r"maven:([A-Za-z0-9_.-]+):([A-Za-z0-9_.-]+):([A-Za-z0-9_.-]+)\Z")
LEGAL_NAME = re.compile(r"(?:.*[-_])?(LICENSE|LICENCE|NOTICE|COPYING|COPYRIGHT)(?:[._-].*)?\Z", re.I)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def repositories(group):
    return (["https://dl.google.com/dl/android/maven2"] if group.startswith("androidx.") or group == "com.google.android.material"
            else ["https://repo.maven.apache.org/maven2"])


def download(url, cache):
    if not url.startswith(("https://dl.google.com/dl/android/maven2/", "https://repo.maven.apache.org/maven2/") + SUPPLEMENTAL_PREFIXES):
        raise ValueError("unapproved-material-origin")
    path = cache / digest(url.encode())
    if path.exists():
        data = path.read_bytes()
        if len(data) > MAX_ARCHIVE:
            raise ValueError("material-download-over-budget")
        return data
    request = urllib.request.Request(url, headers={"User-Agent": "ZNet-Sink-pinned-license-material-audit/1"})
    with urllib.request.urlopen(request, timeout=30) as response:
        if not response.url.startswith(("https://dl.google.com/", "https://repo.maven.apache.org/") + SUPPLEMENTAL_PREFIXES):
            raise ValueError("unapproved-material-redirect")
        data = response.read(MAX_ARCHIVE + 1)
    if len(data) > MAX_ARCHIVE:
        raise ValueError("material-download-over-budget")
    path.write_bytes(data)
    return data


def attribution_prefix(data):
    """Return exact byte range of leading publisher comments, never code text."""
    position = 3 if data.startswith(b"\xef\xbb\xbf") else 0
    start = position
    while True:
        while position < len(data) and data[position:position + 1] in (b" ", b"\t", b"\r", b"\n"):
            position += 1
        if data[position:position + 2] == b"/*":
            end = data.find(b"*/", position + 2)
            if end < 0:
                return None
            position = end + 2
        elif data[position:position + 2] == b"//":
            end = data.find(b"\n", position + 2)
            position = len(data) if end < 0 else end + 1
        else:
            break
    prefix = data[start:position]
    # A documentation comment alone is not legal attribution. Preserve the
    # original whole leading-comment region only when its bytes contain notice.
    if not re.search(rb"copyright|licensed under|license/|licen[cs]e(?:\.txt| agreement)", prefix, re.I):
        return None
    return start, position


def archive_materials(data, url, package_id, sources=False, member_prefix=None):
    archive_sha = digest(data)
    found = {}
    gaps = set()
    scanned = 0
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        members = sorted(archive.infolist(), key=lambda member: member.filename)
        if len(members) > 20000 or sum(member.file_size for member in members) > MAX_TOTAL_EXPANDED:
            raise ValueError("source-archive-over-budget")
        for member in members:
            name = member.filename
            if member.is_dir():
                continue
            if name.startswith("/") or ".." in name.split("/") or "\\" in name:
                raise ValueError("unsafe-source-archive-member")
            if member_prefix and not name.startswith(member_prefix):
                continue
            legal = LEGAL_NAME.fullmatch(name.rsplit("/", 1)[-1])
            source = sources and name.endswith((".java", ".kt", ".kts", ".c", ".h", ".cpp"))
            if not legal and not source:
                continue
            if member.file_size > MAX_TEXT:
                gaps.add("publisher-material-member-over-budget")
                continue
            content = archive.read(member)
            selection = {"kind": "zip-member"}
            if source and not legal:
                scanned += 1
                span = attribution_prefix(content)
                if span is None:
                    continue
                selection = {"kind": "verbatim-leading-source-notice", "startByte": span[0], "endByte": span[1]}
                content = content[span[0]:span[1]]
                kind = "notice"
            else:
                leaf = name.rsplit("/", 1)[-1].upper()
                kind = "notice" if leaf.endswith(".SPDX") or "NOTICE" in leaf else "copyright" if "COPYRIGHT" in leaf else "license"
            try:
                text = content.decode("utf-8", errors="strict")
            except UnicodeDecodeError:
                gaps.add("publisher-material-not-utf8")
                continue
            sha = digest(content)
            key = (kind, sha)
            origin = {"url": url, "sha256": archive_sha, "member": name, "selection": selection}
            if key in found:
                found[key]["occurrences"].append(origin)
            else:
                found[key] = {"name": name + (" [verbatim source notice]" if source and not legal else ""),
                              "kind": kind, "source": package_id + "!publisher-sources!" + name,
                              "sha256": sha, "text": text, "origin": origin, "occurrences": [origin]}
    return sorted(found.values(), key=lambda item: (item["kind"], item["sha256"])), sorted(gaps), scanned


def collect_package(row, inputs, cache):
    package_id = row["id"]
    match = COORDINATE.fullmatch(package_id)
    if not match:
        raise ValueError("invalid-selected-maven-identity")
    group, artifact, version = match.groups()
    selected = sorted((item for item in inputs if item["label"].startswith(package_id + "!") and "!parent-pom:" not in item["label"]), key=lambda item: item["label"])
    selected = [dict(label=label, sha256=sha) for label, sha in sorted({(item["label"], item["sha256"]) for item in selected})]
    artifacts = [item for item in selected if not item["label"].endswith("!pom")]
    poms = [item for item in selected if item["label"].endswith("!pom")]
    expected = {}
    if artifacts:
        artifact_set = [{"name": item["label"].split("!", 1)[1], "sha256": item["sha256"]} for item in artifacts]
        expected["artifactSha256"] = artifact_set[0]["sha256"] if len(artifact_set) == 1 else digest(json.dumps(artifact_set, separators=(",", ":")).encode())
    if len(poms) == 1:
        expected["pomSha256"] = poms[0]["sha256"]
    result = {"identity": {field: row[field] for field in ("ecosystem", "name", "version", "source")},
              "expected": expected, "materials": [], "resolves": [], "gaps": [],
              "coverage": {"sourceArchive": "unavailable", "scannedSourceFiles": 0}}
    if not selected or not any(item["label"] == package_id + "!pom" for item in selected):
        result["gaps"].append("selected-artifact-or-pom-binding-unavailable")
        return result
    base = repositories(group)[0]
    directory = base + "/" + group.replace(".", "/") + "/" + artifact + "/" + version + "/"
    try:
        for item in selected:
            leaf = artifact + "-" + version + ".pom" if item["label"].endswith("!pom") else item["label"].split("!", 1)[1]
            if not re.fullmatch(r"[A-Za-z0-9_.-]+", leaf):
                raise ValueError("invalid-selected-artifact-name")
            # Gradle module metadata may name a variant artifact without the
            # version suffix. Verify the official canonical bytes against the
            # selected digest; never accept a renamed but different artifact.
            if not item["label"].endswith("!pom") and not leaf.startswith(artifact + "-" + version):
                leaf = artifact + "-" + version + Path(leaf).suffix
            actual = download(directory + leaf, cache)
            if digest(actual) != item["sha256"]:
                raise ValueError("selected-publisher-artifact-digest-mismatch")
        source_url = directory + artifact + "-" + version + "-sources.jar"
        data = download(source_url, cache)
        materials, gaps, count = archive_materials(data, source_url, package_id, sources=True)
        result.update(materials=materials, gaps=gaps, coverage={"sourceArchive": "available", "sourceArchiveUrl": source_url, "sourceArchiveSha256": digest(data), "scannedSourceFiles": count})
        if not materials:
            result["gaps"].append("publisher-source-legal-material-not-present")
        if not any(material["kind"] == "license" for material in materials):
            result["gaps"].append("publisher-source-full-license-file-not-present")
    except urllib.error.HTTPError as error:
        result["gaps"].append("official-publisher-material-http-" + str(error.code))
    except (urllib.error.URLError, TimeoutError):
        result["gaps"].append("official-publisher-material-fetch-failed")
    except (ValueError, zipfile.BadZipFile) as error:
        result["gaps"].append(str(error) if isinstance(error, ValueError) else "publisher-source-archive-invalid")
    if artifact in ("kotlinx-coroutines-android", "kotlinx-coroutines-core-jvm") and group == "org.jetbrains.kotlinx" and version == "1.9.0" and result["coverage"]["sourceArchive"] == "available":
        root_url = SUPPLEMENTAL_PREFIXES[0]
        for name, kind in [("LICENSE.txt", "license"), ("license/NOTICE.txt", "notice")]:
            data = download(root_url + name, cache)
            result["materials"].append({"name": name, "kind": kind,
                "source": package_id + "!official-release/" + name, "sha256": digest(data),
                "text": data.decode("utf-8", errors="strict"),
                "origin": {"url": root_url + name, "sha256": digest(data), "revision": COROUTINES_REVISION, "member": name, "selection": {"kind": "raw-publisher-file"}}, "occurrences": []})
        result["coverage"]["officialReleaseRevision"] = COROUTINES_REVISION
        result["resolves"] = ["package-specific-legal-material-not-provided"]
        result["gaps"] = []
    if artifact in ("constraintlayout", "constraintlayout-solver") and group == "androidx.constraintlayout" and version == "2.0.1" and result["gaps"] == ["official-publisher-material-http-404"]:
        # No published sources classifier exists. A reviewed exact official tag
        # archive is a separate, explicitly labelled source evidence route.
        source_url = SUPPLEMENTAL_PREFIXES[1]
        data = download(source_url, cache)
        module = "constraintlayout" if artifact == "constraintlayout" else "solver"
        prefix = "constraintlayout-" + CONSTRAINT_REVISION + "/constraintlayout/" + module + "/src/main/"
        materials, gaps, count = archive_materials(data, source_url, package_id, sources=True, member_prefix=prefix)
        for material in materials:
            material["origin"]["revision"] = CONSTRAINT_REVISION
        result["materials"] = materials
        result["coverage"].update(officialReleaseRevision=CONSTRAINT_REVISION, alternateSourceArchiveUrl=source_url, alternateSourceArchiveSha256=digest(data), scannedSourceFiles=count)
        result["gaps"] = gaps + ["published-sources-classifier-not-present", "publisher-source-full-license-file-not-present"]
    # This marker means missing *package-specific* attribution, not a legal
    # completeness verdict. Only a real publisher copyright/NOTICE can resolve
    # it; an Apache boilerplate appendix is not package copyright evidence.
    if result["materials"] and not any("invalid" in gap or "over-budget" in gap or "not-utf8" in gap for gap in result["gaps"]):
        concrete = [material for material in result["materials"] if material["kind"] in ("notice", "copyright") and re.search(r"copyright\s*(?:\(c\)\s*)?(?:[0-9]{4}|©)|©\s*[0-9]{4}", material["text"], re.I)]
        if concrete:
            result["resolves"] = ["package-specific-legal-material-not-provided"]
            result["coverage"]["packageSpecificAttribution"] = "verbatim-publisher-copyright-or-notice-observed"
    # Keep raw source extraction receipts separately from UI text fields. Duplicate
    # identical headers need only one legal text, with a real publisher origin.
    for material in result["materials"]:
        origin = material["origin"]
        selection = origin.pop("selection")
        if selection["kind"] == "verbatim-leading-source-notice":
            origin["byteRange"] = [selection["startByte"], selection["endByte"]]
        material.pop("occurrences")
        if material["kind"] == "copyright":
            material["kind"] = "notice"
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inventory", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--download-cache", required=True)
    args = parser.parse_args()
    inventory_bytes = Path(args.inventory).read_bytes()
    inventory = json.loads(inventory_bytes)
    if inventory.get("schemaVersion") != 1 or inventory.get("target", {}).get("platform") != "android":
        raise ValueError("Android Gradle inventory required")
    rows = sorted((row for row in inventory["entries"] if row["ecosystem"] == "maven"), key=lambda row: row["id"])
    cache = Path(args.download_cache)
    cache.mkdir(parents=True, exist_ok=True)
    with concurrent.futures.ThreadPoolExecutor(max_workers=6) as executor:
        packages = list(executor.map(lambda row: collect_package(row, inventory["provenance"]["inputs"], cache), rows))
    result = {"schemaVersion": 1, "provenance": {"kind": "reviewed-exact-maven-publisher-materials", "inventorySha256": digest(inventory_bytes)}, "packages": [package for package in packages if package["materials"]], "audit": [{"identity": package["identity"], "coverage": package["coverage"], "gaps": package["gaps"]} for package in packages]}
    Path(args.output).parent.mkdir(parents=True, exist_ok=True)
    Path(args.output).write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print("Android exact publisher material audit:", len(packages), "packages;", sum(bool(package["materials"]) for package in packages), "with source materials;", sum(len(package["gaps"]) for package in packages), "explicit gaps")


if __name__ == "__main__":
    main()
