"""Write THIRD_PARTY_LICENSES.txt: every dependency compiled into the Windows
executables, with its licence and the licence texts it ships. Crates that
declare a licence without shipping its text are covered by the standard
texts collected at the end of the file.

    python tools/third_party_licenses.py [--out PATH]

build.ps1 runs it and writes dist/THIRD_PARTY_LICENSES.txt.
"""

import argparse
import json
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "dist" / "THIRD_PARTY_LICENSES.txt"
# The toolchain the release is built with (its host triple, e.g.
# x86_64-pc-windows-gnu or x86_64-pc-windows-msvc).
TARGET = next(
    line.split(":", 1)[1].strip()
    for line in subprocess.run(["rustc", "-vV"], capture_output=True, text=True, check=True).stdout.splitlines()
    if line.startswith("host:")
)
PREFIXES = ("LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE", "OFL", "UFL")

# Licence ids whose standard text is appended, and file names that hold it.
STANDARD = {
    "Apache-2.0": ("LICENSE-APACHE", "LICENSE-APACHE2", "LICENSE-APACHE-2.0"),
    "MIT": ("LICENSE-MIT",),
    "BSD-3-Clause": ("LICENSE-BSD", "LICENSE-BSD-3-CLAUSE"),
    "BSL-1.0": ("LICENSE-BOOST", "LICENSE-BSL"),
    "MPL-2.0": ("LICENSE-MPL", "LICENSE-MPL-2.0"),
    "Zlib": ("LICENSE-ZLIB",),
}


def licence_files(crate_dir: pathlib.Path) -> list[pathlib.Path]:
    found = [f for f in crate_dir.iterdir() if f.is_file() and f.name.upper().startswith(PREFIXES)]
    if not found:
        # Some crates keep them in a subfolder (fonts/, licenses/).
        found = [
            f for f in crate_dir.rglob("*")
            if f.is_file() and f.name.upper().startswith(PREFIXES) and f.suffix.lower() in ("", ".txt", ".md")
        ]
    return sorted(found)


def main() -> None:
    global OUT
    args = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    args.add_argument("--out", type=pathlib.Path, default=OUT)
    OUT = args.parse_args().out.resolve()
    OUT.parent.mkdir(parents=True, exist_ok=True)
    meta = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--format-version", "1", "--filter-platform", TARGET], cwd=ROOT
        )
    )
    used = {n["id"] for n in meta["resolve"]["nodes"]}
    packages = sorted(
        (p for p in meta["packages"] if p["id"] in used and p["source"]),
        key=lambda p: (p["name"], p["version"]),
    )
    parts = [
        "Third-party software in ShoutingMonkey",
        "=" * 38,
        "",
        "ShoutingMonkey is built from the open-source packages below. Each is",
        "listed with its licence, followed by the licence texts it ships. Where a",
        "package names a licence without shipping its text, the standard text is",
        "in the last section of this file.",
        "",
    ]
    standard: dict[str, str] = {}
    missing = []
    for p in packages:
        crate_dir = pathlib.Path(p["manifest_path"]).parent
        texts = licence_files(crate_dir)
        for lic, names in STANDARD.items():
            # A crate under exactly this licence: its plain LICENSE file is the text.
            single = (p.get("license") or "").strip() == lic
            for f in texts:
                named = f.name.upper() in {n.upper() for n in names}
                plain = single and f.stem.upper() in ("LICENSE", "LICENCE")
                if lic not in standard and (named or plain):
                    standard[lic] = f.read_text(encoding="utf-8", errors="replace").strip()
        parts.append("=" * 78)
        parts.append(f"{p['name']} {p['version']}")
        parts.append(f"Licence: {p.get('license') or 'see below'}")
        if p.get("repository"):
            parts.append(f"Source: {p['repository']}")
        if not texts:
            missing.append(f"{p['name']} {p['version']} ({p.get('license')})")
        for t in texts:
            parts.append("")
            parts.append(f"[{t.relative_to(crate_dir).as_posix()}]")
            parts.append(t.read_text(encoding="utf-8", errors="replace").strip())
        parts.append("")
    parts.append("=" * 78)
    parts.append("Standard licence texts")
    parts.append("")
    for lic, text in sorted(standard.items()):
        parts.append(f"--- {lic} ---")
        parts.append(text)
        parts.append("")
    OUT.write_text("\n".join(parts) + "\n", encoding="utf-8")
    print(f"wrote {OUT.name}: {len(packages)} packages, standard texts: {', '.join(sorted(standard))}")
    if missing:
        print(f"{len(missing)} ship no licence file; their licence is covered by the standard texts:")
        for m in missing:
            print("  " + m)


if __name__ == "__main__":
    main()
