"""Collect notices from the exact Cargo metadata graph into a distributable text file."""
import json
import pathlib
import sys

metadata = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8-sig"))
packages = sorted(metadata["packages"], key=lambda p: (p["name"], p["version"]))
chunks = ["D101 Label Studio: Rust dependency license notices\n"]
for package in packages:
    if not package.get("source"):
        continue
    root = pathlib.Path(package["manifest_path"]).parent
    chunks.append(f"\n{'='*72}\n{package['name']} {package['version']}\nSPDX: {package.get('license')}\n{package.get('repository') or ''}\n")
    paths = [p for p in root.iterdir() if p.is_file() and
             (p.name.lower().startswith(("license", "licence", "copying", "notice")))]
    if package.get("license_file"):
        candidate = root / package["license_file"]
        if candidate.is_file() and candidate not in paths:
            paths.append(candidate)
    for path in sorted(paths):
        chunks.append(f"\n--- {path.name} ---\n{path.read_text(encoding='utf-8', errors='replace')}\n")
pathlib.Path(sys.argv[2]).write_text("".join(chunks), encoding="utf-8")
print(f"Collected notices for {len(packages)} resolved packages")
