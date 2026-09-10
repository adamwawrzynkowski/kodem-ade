#!/usr/bin/env python3
"""Refresh reviewed Rust notices, or reject stale notices before packaging."""
import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]
TOOL_VERSION = "0.9.2"
TARGET = "aarch64-apple-darwin"
NOTICE = "assets/licenses/Rust-THIRD-PARTY.txt"
SOURCE = "assets/licenses/option-ext-0.2.0-source.tar.gz"
STANDARD_LIBRARY = "assets/licenses/Rust-standard-library.html"
OUTPUTS = [NOTICE, SOURCE, STANDARD_LIBRARY]
SNAPSHOT = ROOT / "licensing/snapshot.json"


def digest(path):
    """Return a file's SHA-256 digest."""
    return hashlib.sha256(path.read_bytes()).hexdigest()


def inputs():
    """Hash dependency declarations and the reviewed licensing inputs."""
    paths = [ROOT / p for p in ["Cargo.lock", "Cargo.toml", "rust-toolchain.toml", "about.toml", "LICENSE", "scripts/licenses.py", "assets/THIRD_PARTY.md"]]
    paths += list((ROOT / "crates").glob("*/Cargo.toml"))
    paths += [p for p in (ROOT / "licensing").rglob("*") if p.is_file() and p != SNAPSHOT]
    paths += [p for p in (ROOT / "assets/licenses").rglob("*") if p.is_file() and str(p.relative_to(ROOT)) not in OUTPUTS]
    return {str(p.relative_to(ROOT)): digest(p) for p in sorted(paths)}


def check():
    """Reject stale or altered notice artifacts without network access."""
    saved = json.loads(SNAPSHOT.read_text())
    current = inputs()
    changed = sorted(k for k in set(current) | set(saved["inputs"]) if current.get(k) != saved["inputs"].get(k))
    for name, expected in saved["outputs"].items():
        path = ROOT / name
        if not path.is_file() or digest(path) != expected:
            changed.append(name)
    if changed:
        raise SystemExit("Licensing snapshot is stale: " + ", ".join(changed) + "\nReview dependency licenses, then run python3 scripts/licenses.py refresh.")
    print("License notices verified for {} external packages ({})".format(saved["package_count"], TARGET))


def source_archive(package):
    """Bundle the complete unmodified MPL crate with reproducible archive metadata."""
    root = Path(package["manifest_path"]).parent
    buffer = io.BytesIO()
    with gzip.GzipFile(fileobj=buffer, mode="wb", mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode="w") as archive:
            for path in sorted(root.rglob("*")):
                if not path.is_file() or path.name == ".cargo-ok":
                    continue
                data = path.read_bytes()
                info = tarfile.TarInfo("option-ext-0.2.0/" + str(path.relative_to(root)))
                info.size = len(data)
                info.mode = 0o644
                archive.addfile(info, io.BytesIO(data))
    return buffer.getvalue()


def refresh():
    """Generate locked notices with cargo-about and version-specific reviewed clarifications."""
    tool = os.environ.get("KODEM_CARGO_ABOUT") or shutil.which("cargo-about")
    if not tool:
        raise SystemExit("Install cargo-about: cargo install cargo-about --version " + TOOL_VERSION + " --locked")
    version = subprocess.check_output([tool, "--version"], text=True).strip()
    if version != "cargo-about " + TOOL_VERSION:
        raise SystemExit("Expected cargo-about " + TOOL_VERSION + "; found " + version)
    with tempfile.TemporaryDirectory(prefix="kodem-licenses-") as temporary:
        output = Path(temporary) / "licenses.json"
        subprocess.run([tool, "generate", "--workspace", "--locked", "--offline", "--fail", "--format", "json", "--target", TARGET, "--output-file", str(output)], cwd=ROOT, check=True)
        result = json.loads(output.read_text())
    overrides = json.loads((ROOT / "licensing/clarifications.json").read_text())
    packages = {p["package"]["id"]: p["package"] for p in result["crates"]}
    if any(p["source"] is None for p in packages.values()):
        raise SystemExit("Private workspace crates must be excluded from third-party notices")
    groups = {}
    for license_entry in result["licenses"]:
        for entry in license_entry["used_by"]:
            package = entry["crate"]
            key = package["name"] + "@" + package["version"]
            text = license_entry["text"]
            if "Copyright (c) <year>" in text:
                clarification = overrides.get(key)
                if not clarification:
                    raise SystemExit("Unreviewed placeholder license for " + key)
                if clarification.get("text"):
                    text = clarification["text"]
                else:
                    # The reviewed upstream declaration supplies no copyright line.
                    # Preserve its authors below; do not invent a copyright claim.
                    text = text.replace("Copyright (c) <year> <copyright holders>\n", "")
                text += "\n\nLicense evidence: " + clarification["source"] + "\n"
                text += clarification.get("declaration", "")
                if "Copyright (c) <year>" in text:
                    raise SystemExit("Unresolved copyright placeholder for " + key)
            groups.setdefault((license_entry["id"], text), {})[key] = package
    lines = ["Rust dependency notices", "", "Target: " + TARGET, "Generated by cargo-about " + TOOL_VERSION + ". Build-only and development dependencies are excluded.",
             "License alternatives shown below are selected by about.toml. Additional upstream notices are preserved afterward.",
             "", "MPL source: the complete, unmodified option-ext 0.2.0 source is included alongside this file as option-ext-0.2.0-source.tar.gz.",
             "That source remains under MPL-2.0. Kodem's proprietary terms do not restrict your rights to it.", ""]
    for (identifier, text), users in sorted(groups.items()):
        lines += ["=" * 72, identifier, ""]
        for key, package in sorted(users.items()):
            lines.append(key + " | " + (package["repository"] or "https://crates.io/crates/" + package["name"]))
            if package["authors"]:
                lines.append("Upstream authors: " + "; ".join(package["authors"]))
        lines += ["", text.strip(), ""]
    # Keep complete upstream copyright and NOTICE files, including notices that
    # license detection normalizes or that belong to an alternative license.
    notices = {}
    for package in packages.values():
        root = Path(package["manifest_path"]).parent
        for path in sorted(root.iterdir()):
            if path.is_file() and path.name.lower().startswith(("license", "licence", "copying", "copyright", "notice", "authors")):
                text = path.read_text(errors="replace").strip()
                notices.setdefault(text, []).append(package["name"] + "@" + package["version"] + "/" + path.name)
    lines += ["=" * 72, "Additional original upstream notices", ""]
    for text, names in sorted(notices.items(), key=lambda item: sorted(item[1])):
        lines += ["; ".join(sorted(names)), "", text, ""]
    mpl = [p for p in packages.values() if p["license"] == "MPL-2.0"]
    if [(p["name"], p["version"]) for p in mpl] != [("option-ext", "0.2.0")]:
        raise SystemExit("Review MPL source packaging for the changed dependency set")
    sysroot = Path(subprocess.check_output(["rustc", "--print", "sysroot"], cwd=ROOT, text=True).strip())
    standard_library = (sysroot / "share/doc/rust/COPYRIGHT-library.html").read_bytes()
    notice_bytes = ("\n".join(lines).rstrip() + "\n").encode()
    archive_bytes = source_archive(mpl[0])
    (ROOT / STANDARD_LIBRARY).write_bytes(standard_library)
    (ROOT / NOTICE).write_bytes(notice_bytes)
    (ROOT / SOURCE).write_bytes(archive_bytes)
    snapshot = {"tool_version": TOOL_VERSION, "target": TARGET, "package_count": len(packages), "inputs": inputs(), "outputs": {p: digest(ROOT / p) for p in OUTPUTS}}
    SNAPSHOT.write_text(json.dumps(snapshot, indent=2, sort_keys=True) + "\n")
    check()


def main():
    """Run the requested notice refresh or offline packaging check."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["refresh", "check"])
    parser.add_argument("--target", default=TARGET, help="The application build target")
    args = parser.parse_args()
    if args.target != TARGET:
        parser.error("No reviewed licensing policy for target " + args.target)
    if args.command == "refresh":
        refresh()
    else:
        check()


if __name__ == "__main__":
    main()
