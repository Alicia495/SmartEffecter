"""Build release archives from a clean Git commit and the current dist binary.

Run after scripts/build.ps1 in the same configured developer environment.
No credentials or local user settings are included.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parent.parent


def run(*args, cwd=ROOT, env=None):
    return subprocess.check_output(args, cwd=cwd, env=env, text=True, encoding="utf-8")


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def main():
    if run("git", "status", "--porcelain").strip():
        raise SystemExit("Commit all changes before packaging the release.")
    version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
    tag = "v" + version.removesuffix(".0")
    output = ROOT / "artifacts" / "releases" / tag
    output.mkdir(parents=True, exist_ok=True)
    stage = output / "stage"
    if stage.exists():
        raise SystemExit(f"Staging directory already exists: {stage}")
    app = stage / f"SmartEffecter-{tag}"
    app.mkdir(parents=True)
    for name in ["GuitarSmartEffecter.exe", "README.md", "LICENSE", "CHANGELOG.md", "TEST_REPORT.md", "THIRD_PARTY_NOTICES.md"]:
        shutil.copy2(ROOT / "dist" / name, app / name)
    shutil.copytree(ROOT / "docs", app / "docs")
    shutil.copytree(ROOT / "dist" / "licenses", app / "licenses")
    app_zip = output / f"SmartEffecter-{tag}-windows-x64.zip"
    with zipfile.ZipFile(app_zip, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
        for path in sorted(app.rglob("*")):
            if path.is_file():
                archive.write(path, path.relative_to(stage))

    source = stage / f"SmartEffecter-{tag}-source"
    source.mkdir()
    for name in run("git", "ls-files", "-z").split("\0"):
        if name:
            destination = source / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / name, destination)
    metadata = json.loads(run("cargo", "metadata", "--locked", "--offline", "--format-version", "1"))
    dependencies = [p for p in metadata["packages"] if p["source"]]
    for package in dependencies:
        origin = Path(package["manifest_path"]).parent
        destination = source / "vendor" / f"{package['name']}-{package['version']}"
        shutil.copytree(origin, destination)
        checksum = destination / ".cargo-checksum.json"
        original = json.loads(checksum.read_text(encoding="utf-8"))
        files = {p.relative_to(destination).as_posix(): digest(p)
                 for p in sorted(destination.rglob("*")) if p.is_file() and p != checksum}
        checksum.write_text(json.dumps({"files": files, "package": original["package"]}), encoding="utf-8")
    sdk = Path(os.environ.get("CPAL_ASIO_DIR", ROOT / ".tools/asio-sdk/ASIOSDK"))
    sdk_destination = source / "third_party" / "ASIOSDK"
    sdk_destination.mkdir(parents=True)
    for name in ["common", "host"]:
        shutil.copytree(sdk / name, sdk_destination / name)
    shutil.copy2(sdk / "LICENSE.txt", sdk_destination / "LICENSE.txt")
    shutil.copy2(ROOT / "LICENSE", sdk_destination / "GPL-3.0.txt")
    config = source / ".cargo" / "config.toml"
    config.parent.mkdir(parents=True, exist_ok=True)
    config.write_text('[source.crates-io]\nreplace-with = "vendored-sources"\n\n[source.vendored-sources]\ndirectory = "vendor"\n', encoding="utf-8")
    manifest = {"version": version, "tag": tag, "commit": run("git", "rev-parse", "HEAD").strip(),
                "rustc": run("rustc", "-Vv"), "dependency_packages": len(dependencies),
                "executable_sha256": digest(app / "GuitarSmartEffecter.exe")}
    (source / "RELEASE_BUILD.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    # Verify dependency resolution without a registry cache or network access.
    check_env = os.environ.copy()
    check_env["CARGO_HOME"] = str(output / "empty-cargo-home")
    run("cargo", "metadata", "--offline", "--locked", "--format-version", "1", cwd=source, env=check_env)
    source_tar = output / f"SmartEffecter-{tag}-source.tar.gz"
    with tarfile.open(source_tar, "w:gz", compresslevel=6) as archive:
        archive.add(source, arcname=source.name)
    checksums = output / "SHA256SUMS.txt"
    checksums.write_text("".join(f"{digest(p)}  {p.name}\n" for p in [app_zip, source_tar]), encoding="utf-8")
    print(json.dumps({**manifest, "assets": [{"path": str(p), "bytes": p.stat().st_size} for p in [app_zip, source_tar, checksums]]}, indent=2))


if __name__ == "__main__":
    main()
