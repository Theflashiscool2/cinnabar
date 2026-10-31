#!/usr/bin/env python3
"""Adds a user's own compiled carriers and vanilla resource pack to an unsigned Cinnabar IPA.

The CI IPA carries no Mojang-derived files. This copies every entry of that IPA into a new one
and adds `<local>/assets/compiled/**` and the installed vanilla pack below the app's resource
root at the same `.local`-relative paths the client reads on desktop. Entries already at those
paths are replaced, so an IPA produced by this script can be refreshed. The IPA stays unsigned;
sideloading tools sign the bundle.
"""

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import shutil
import stat
import sys
import tempfile
import zipfile


ROOT = Path(__file__).resolve().parents[2]
VANILLA_SOURCE = ROOT / "assets/vanilla-source.json"
RUNTIME = Path(__file__).resolve().with_name("runtime.json")
COMPILED = "assets/compiled"
JAVA_TARGET = ROOT / "crates/java-protocol/data/target.json"


def resource_dir():
    """The bundle directory below `<App>.app/` that mirrors `.local/`, as runtime.json names it."""
    return json.loads(RUNTIME.read_text(encoding="utf-8"))["resource_dir"]


class AssetError(Exception):
    """A missing input or refused output, reported without a traceback."""


def installed_pack_dir(pack):
    """The pack's directory below a resource root, as `VanillaSource::installed_pack_dir` builds it."""
    cache = json.loads(VANILLA_SOURCE.read_text(encoding="utf-8"))["cache_dir"]
    return f"{cache.removeprefix('.local/')}/{pack}"


def app_dir(names):
    """The single `Payload/<App>.app/` prefix among the IPA's entry names."""
    apps = {
        "/".join(parts[:2]) + "/"
        for parts in (PurePosixPath(name).parts for name in names)
        if len(parts) >= 2 and parts[0] == "Payload" and parts[1].endswith(".app")
    }
    if len(apps) != 1:
        raise AssetError(f"expected one Payload/<App>.app in the IPA, found {sorted(apps) or 'none'}")
    return apps.pop()


def source_dirs(local):
    """The `.local`-relative directories to inject, each required to hold at least one file."""
    dirs = [COMPILED, installed_pack_dir("resource_pack")]
    for relative in dirs:
        directory = local / relative
        if not directory.is_dir() or not any(path.is_file() for path in directory.rglob("*")):
            raise AssetError(
                f"missing {directory}: run `make assets` in the checkout to compile the carriers "
                "and install the vanilla resource pack"
            )
    return dirs


def copy_entry(source, target, info):
    """Streams one IPA entry, keeping its name, timestamp and unix mode bits."""
    entry = zipfile.ZipInfo(info.filename, info.date_time)
    entry.external_attr = info.external_attr
    entry.create_system = info.create_system
    if info.is_dir():
        target.writestr(entry, b"")
        return
    entry.compress_type = zipfile.ZIP_DEFLATED
    entry.file_size = info.file_size
    with source.open(info) as reader, target.open(entry, "w") as writer:
        shutil.copyfileobj(reader, writer, 1 << 20)


def add_file(target, path, name, expected=None):
    """Streams one local file into the IPA as a regular 0644 unix entry."""
    entry = zipfile.ZipInfo.from_file(path, name, strict_timestamps=False)
    entry.external_attr = (stat.S_IFREG | 0o644) << 16
    entry.create_system = 3
    entry.compress_type = zipfile.ZIP_DEFLATED
    digest = hashlib.sha256()
    size = 0
    with open(path, "rb") as reader, target.open(entry, "w") as writer:
        while chunk := reader.read(1 << 20):
            writer.write(chunk)
            if expected is not None:
                digest.update(chunk)
                size += len(chunk)
    if expected is not None and (size != expected["bytes"] or digest.hexdigest() != expected["sha256"]):
        raise AssetError(f"{path.name}: Java artifact does not match pinned size/SHA-256; output not replaced")


def java_sources(directory):
    """Only pinned runtime inputs enter the app; exporters, credentials and server jars stay out."""
    manifest = json.loads(JAVA_TARGET.read_text(encoding="utf-8"))
    relative = f"assets/java/{manifest['game_version']}"
    specs = [("client.jar", manifest["client_assets"])]
    for key in ("block_registry_report", "physics_report", "interaction_report", "entity_models_report"):
        spec = manifest[key]
        specs.append((PurePosixPath(spec["path"]).name, spec))
    for name, _ in specs:
        if not (directory / name).is_file():
            raise AssetError(f"missing Java runtime input {directory / name}; prepare pinned exports on the host")
    return relative, specs


def add_assets(ipa, local, out, force=False, java_assets=None):
    """Writes `out`: `ipa` plus the local carriers and pack. Returns the number of files added."""
    if not ipa.is_file():
        raise AssetError(f"missing IPA {ipa}")
    if out.exists() and not force:
        raise AssetError(f"{out} exists; pass --force to replace it")
    dirs = source_dirs(local)
    if java_assets is None:
        version = json.loads(JAVA_TARGET.read_text(encoding="utf-8"))["game_version"]
        prepared = local / "assets/java" / version
        if prepared.exists():
            java_assets = prepared
    java_relative, java_specs = java_sources(java_assets) if java_assets is not None else (None, [])
    added = 0
    with tempfile.NamedTemporaryFile(dir=out.parent, prefix=out.name, suffix=".tmp", delete=False) as handle:
        partial = Path(handle.name)
    try:
        with zipfile.ZipFile(ipa) as source, zipfile.ZipFile(partial, "w", zipfile.ZIP_DEFLATED) as target:
            resources = f"{app_dir(source.namelist())}{resource_dir()}/"
            replaced = tuple(f"{resources}{relative}/" for relative in dirs + ([java_relative] if java_relative else []))
            for info in source.infolist():
                if not info.filename.startswith(replaced):
                    copy_entry(source, target, info)
            for relative in dirs:
                for path in sorted((local / relative).rglob("*")):
                    if path.is_file():
                        add_file(target, path, resources + path.relative_to(local).as_posix())
                        added += 1
            for name, expected in java_specs:
                add_file(target, java_assets / name, f"{resources}{java_relative}/{name}", expected)
                added += 1
        partial.replace(out)
    except BaseException:
        partial.unlink(missing_ok=True)
        raise
    return added


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("ipa", type=Path, help="unsigned IPA from build-ipa.sh or the iOS workflow")
    parser.add_argument("--local", type=Path, default=ROOT / ".local", help="checkout .local directory")
    parser.add_argument("--out", type=Path, help="output IPA (default: -unsigned replaced by -assets)")
    parser.add_argument("--java-assets", type=Path, help="prepared Java runtime directory (default: use local/assets/java/<pinned-version> when present); verified against the tracked target manifest")
    parser.add_argument("--force", action="store_true", help="replace an existing output")
    args = parser.parse_args(argv)
    out = args.out
    if out is None:
        stem = args.ipa.stem.removesuffix("-unsigned")
        out = args.ipa.with_name(f"{stem}-assets{args.ipa.suffix}")
    if out.resolve() == args.ipa.resolve():
        parser.error("the output must differ from the input IPA")
    try:
        added = add_assets(args.ipa, args.local, out, args.force, args.java_assets)
    except AssetError as error:
        print(f"add-assets: {error}", file=sys.stderr)
        return 1
    print(f"{out} ({out.stat().st_size / (1 << 20):.1f} MiB, {added} files added)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
