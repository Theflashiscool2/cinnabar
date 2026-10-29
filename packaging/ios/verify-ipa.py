#!/usr/bin/env python3
"""Inspect an iPhone archive without extracting or executing it; this is not a launch test."""

import argparse
import hashlib
import json
from pathlib import Path
import plistlib
import re
import struct
import sys
import zipfile


ROOT = Path(__file__).resolve().parents[2]


def verify(path, require_assets=False):
    """Require scene metadata, an arm64 device executable and the packaged startup resources."""
    runtime = json.loads((ROOT / "packaging/ios/runtime.json").read_text())
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        if len(names) != len(set(names)):
            raise ValueError("duplicate archive entries")
        plists = [name for name in names if re.fullmatch(r"Payload/[^/]+\.app/Info\.plist", name)]
        if len(plists) != 1:
            raise ValueError("expected one application Info.plist")
        prefix = plists[0].removesuffix("Info.plist")
        info = plistlib.loads(archive.read(plists[0]))
        manifest = info.get("UIApplicationSceneManifest", {})
        configs = manifest.get("UISceneConfigurations", {})
        scenes = configs.get("UIWindowSceneSessionRoleApplication", [])
        if (manifest.get("UIApplicationSupportsMultipleScenes") is not False
                or set(configs) != {"UIWindowSceneSessionRoleApplication"}
                or len(scenes) != 1
                or scenes[0].get("UISceneClassName") != "UIWindowScene"
                or not scenes[0].get("UISceneConfigurationName")
                or "UISceneStoryboardFile" in scenes[0]
                or "UISceneDelegateClassName" in scenes[0]):
            raise ValueError("expected the single UIKit scene owned by patched winit")
        if info.get("CFBundleVersion") != runtime["build_number"]:
            raise ValueError("stale iOS build number")
        commit = info.get("CinnabarSourceCommit", "")
        if not re.fullmatch(r"[0-9a-f]{40}(?:-dirty)?", commit):
            raise ValueError("missing source commit")
        if info.get("CFBundleExecutable") != runtime["executable"]:
            raise ValueError("unexpected executable")
        if info.get("MinimumOSVersion") != runtime["deployment_target"]:
            raise ValueError("unexpected deployment target")
        if info.get("DTPlatformVersion") != runtime["xcode"] and not info.get(
                "DTPlatformVersion", "").startswith(runtime["xcode"] + "."):
            raise ValueError("unexpected SDK version")
        binary_name = prefix + runtime["executable"]
        with archive.open(binary_name) as binary:
            header = binary.read(32)
            if len(header) != 32:
                raise ValueError("truncated executable")
            magic, cpu, _, kind, count, size, _, _ = struct.unpack("<8I", header)
            if (magic, cpu, kind) != (0xFEEDFACF, 0x0100000C, 2):
                raise ValueError("expected a thin arm64 Mach-O executable")
            if size > 1024 * 1024 or count > size // 8:
                raise ValueError("invalid Mach-O load commands")
            commands = binary.read(size)
            offset = 0
            platforms = []
            for _ in range(count):
                if offset + 8 > len(commands):
                    raise ValueError("truncated Mach-O commands")
                command, length = struct.unpack_from("<2I", commands, offset)
                if length < 8 or offset + length > len(commands):
                    raise ValueError("invalid Mach-O command length")
                if command == 0x32:  # LC_BUILD_VERSION
                    if length < 24:
                        raise ValueError("truncated LC_BUILD_VERSION")
                    platforms.append(struct.unpack_from("<I", commands, offset + 8)[0])
                offset += length
            if platforms != [2]:  # PLATFORM_IOS; simulator is 7
                raise ValueError("executable is not built for an iOS device")
        resources = prefix + runtime["resource_dir"] + "/"
        font = json.loads((ROOT / "assets/cinnangles-sans-source.json").read_text())["font_file"]
        for relative in ("fonts/" + font, "licenses/Cinnabar-LICENSE.md"):
            if archive.getinfo(resources + relative).file_size == 0:
                raise ValueError("empty bundled resource: " + relative)
        if require_assets and not any(name.startswith(resources + "assets/compiled/")
                                      and not name.endswith("/") for name in names):
            raise ValueError("no compiled carriers; inject local assets before installing")
    hasher = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(1 << 20):
            hasher.update(chunk)
    digest = hasher.hexdigest()
    return {"ipa": str(path), "sha256": digest, "source_commit": commit,
            "version": info.get("CFBundleShortVersionString"), "build": info["CFBundleVersion"],
            "scene": scenes[0]["UISceneConfigurationName"], "platform": "iPhoneOS arm64",
            "launch_tested": False, "signing": "not assessed; sign with your installation tool"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("ipa", type=Path)
    parser.add_argument("--require-assets", action="store_true")
    args = parser.parse_args()
    try:
        print(json.dumps(verify(args.ipa, args.require_assets), indent=2))
    except (ValueError, KeyError, OSError, zipfile.BadZipFile) as error:
        print(f"verify-ipa: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
