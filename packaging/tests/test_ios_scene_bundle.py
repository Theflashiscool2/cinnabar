"""Exercise final-bundle validation with synthetic Mach-O headers, not an iOS runtime."""

import importlib.util
import json
from pathlib import Path
import plistlib
import struct
import tempfile
import unittest
import zipfile


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("verify_ipa", ROOT / "packaging/ios/verify-ipa.py")
VERIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)
RUNTIME = json.loads((ROOT / "packaging/ios/runtime.json").read_text())


class SceneBundleTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name) / "candidate.ipa"
        # Render the real packaging template, then inspect the archived plist (not source strings).
        text = (ROOT / "packaging/ios/Info.plist.in").read_text()
        values = {"EXECUTABLE": RUNTIME["executable"], "BUNDLE_ID": RUNTIME["bundle_id"],
                  "PRODUCT_NAME": "Cinnabar", "VERSION": "0.1.31",
                  "BUILD_NUMBER": RUNTIME["build_number"], "SOURCE_COMMIT": "a" * 40,
                  "DEPLOYMENT_TARGET": RUNTIME["deployment_target"], "SDK_VERSION": RUNTIME["xcode"]}
        for name, value in values.items():
            text = text.replace("@" + name + "@", value)
        self.info = plistlib.loads(text.encode())

    def write_bundle(self, platform=2, assets=True, executable=True):
        prefix = "Payload/Cinnabar.app/"
        resources = prefix + RUNTIME["resource_dir"] + "/"
        font = json.loads((ROOT / "assets/cinnangles-sans-source.json").read_text())["font_file"]
        with zipfile.ZipFile(self.path, "w") as archive:
            archive.writestr(prefix + "Info.plist", plistlib.dumps(self.info))
            if executable:
                header = struct.pack("<8I", 0xFEEDFACF, 0x0100000C, 0, 2, 1, 24, 0, 0)
                command = struct.pack("<6I", 0x32, 24, platform, 0x110000, 0x1B0100, 0)
                archive.writestr(prefix + RUNTIME["executable"], header + command)
            archive.writestr(resources + "fonts/" + font, b"fixture font")
            archive.writestr(resources + "licenses/Cinnabar-LICENSE.md", b"fixture license")
            if assets:
                archive.writestr(resources + "assets/compiled/fixture.bin", b"fixture carrier")

    def test_single_scene_device_bundle_reports_identity_without_claiming_launch(self):
        self.write_bundle()
        result = VERIFY.verify(self.path, require_assets=True)
        self.assertEqual(result["build"], RUNTIME["build_number"])
        self.assertEqual(result["source_commit"], "a" * 40)
        self.assertEqual(len(result["sha256"]), 64)
        self.assertFalse(result["launch_tested"])

    def test_crashing_legacy_bundle_is_rejected(self):
        del self.info["UIApplicationSceneManifest"]
        self.write_bundle()
        with self.assertRaisesRegex(ValueError, "single UIKit scene"):
            VERIFY.verify(self.path)

    def test_multiple_scenes_and_competing_controller_are_rejected(self):
        scene = self.info["UIApplicationSceneManifest"]
        scene["UIApplicationSupportsMultipleScenes"] = True
        self.write_bundle()
        with self.assertRaises(ValueError):
            VERIFY.verify(self.path)
        scene["UIApplicationSupportsMultipleScenes"] = False
        scene["UISceneConfigurations"]["UIWindowSceneSessionRoleApplication"][0]["UISceneStoryboardFile"] = "Other"
        self.write_bundle()
        with self.assertRaises(ValueError):
            VERIFY.verify(self.path)

    def test_simulator_cannot_be_mistaken_for_device_build(self):
        self.write_bundle(platform=7)
        with self.assertRaisesRegex(ValueError, "iOS device"):
            VERIFY.verify(self.path)

    def test_missing_executable_and_assets_are_reported(self):
        self.write_bundle(executable=False)
        with self.assertRaises(KeyError):
            VERIFY.verify(self.path)
        self.write_bundle(assets=False)
        VERIFY.verify(self.path)
        with self.assertRaisesRegex(ValueError, "no compiled carriers"):
            VERIFY.verify(self.path, require_assets=True)

    def test_old_build_is_rejected(self):
        self.info["CFBundleVersion"] = "0.1.31"
        self.write_bundle()
        with self.assertRaisesRegex(ValueError, "stale"):
            VERIFY.verify(self.path)


if __name__ == "__main__":
    unittest.main()
