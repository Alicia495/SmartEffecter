"""Collect the locked dependency notices without downloading extra packages."""
import json
import os
from pathlib import Path
import shutil
import subprocess

root = Path(__file__).resolve().parent.parent
metadata = json.loads(subprocess.check_output([
    "cargo", "metadata", "--locked", "--offline", "--format-version", "1",
    "--filter-platform", "x86_64-pc-windows-msvc",
], cwd=root, encoding="utf-8"))
dist = root / "dist"
dist.mkdir(exist_ok=True)
lines = [
    "# Third-party notices", "",
    "This inventory includes runtime and build dependencies from Cargo.lock.",
    "License expressions and repository URLs below come from package metadata.",
    "Bundled license/copyright texts are in the licenses directory.", "",
    "The desktop UI uses Slint under its Royalty-free Desktop, Mobile, and Web",
    "Applications License 2.0. Its AboutSlint widget is available via Settings > Info.",
    "ASIO is a trademark and software of Steinberg Media Technologies GmbH.", "",
    "| Package | Declared license | Source |",
    "|---|---|---|",
]
for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
    if package["name"] == "guitar-smart-effecter":
        continue
    source = Path(package["manifest_path"]).parent
    destination = dist / "licenses" / (package["name"] + "-" + package["version"])
    candidates = set()
    for path in source.iterdir():
        if path.name.lower().startswith(("license", "licence", "copying", "copyright", "notice")):
            if path.is_file():
                candidates.add(path)
            elif path.is_dir():
                candidates.update(p for p in path.rglob("*") if p.is_file())
    if package.get("license_file"):
        candidates.add(source / package["license_file"])
    for path in sorted(candidates):
        if not path.is_file():
            continue
        target = destination / path.relative_to(source)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(path, target)
    license_name = (package.get("license") or "See package license file").replace("|", "\\|")
    repository = package.get("repository") or package.get("homepage") or ""
    link = f"[repository]({repository})" if repository else ""
    lines.append(f"| {package['name']} {package['version']} | {license_name} | {link} |")

sdk = Path(os.environ.get("CPAL_ASIO_DIR", root / ".tools/asio-sdk/ASIOSDK"))
for source in [sdk / "LICENSE.txt", sdk / "common/LICENSE.txt", sdk / "Steinberg ASIO Licensing Agreement.pdf"]:
    if source.is_file():
        destination = dist / "licenses/ASIO-SDK" / source.relative_to(sdk)
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)
(dist / "THIRD_PARTY_NOTICES.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
print(f"Dependency notices: {len(metadata['packages']) - 1} packages")
