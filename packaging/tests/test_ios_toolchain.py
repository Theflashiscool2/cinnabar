"""Exercise build-script preflight with command fixtures, not an Apple SDK or compiler."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class ToolchainTests(unittest.TestCase):
    def run_preflight(self, xcode, sdk, developer=None):
        with tempfile.TemporaryDirectory() as directory:
            bin_dir = Path(directory)
            scripts = {
                "uname": "printf 'Darwin\\n'",
                "xcodebuild": "printf 'Xcode %s\\nBuild version fixture\\n' \"$FIXTURE_XCODE\"",
                "xcrun": '''case "$*" in
                    *--show-sdk-version) printf '%s\\n' "$FIXTURE_SDK" ;;
                    *--show-sdk-path) printf '/fixture/SDK\\n' ;;
                    *) exit 91 ;;
                esac''',
                "go": "exit 92",
                "cargo": "exit 93",
            }
            for name, body in scripts.items():
                path = bin_dir / name
                path.write_text("#!/bin/sh\n" + body + "\n")
                path.chmod(0o755)
            env = dict(os.environ, PATH=str(bin_dir) + os.pathsep + os.environ["PATH"],
                       FIXTURE_XCODE=xcode, FIXTURE_SDK=sdk)
            env.pop("DEVELOPER_DIR", None)
            if developer:
                env["DEVELOPER_DIR"] = developer
            return subprocess.run(["bash", str(ROOT / "packaging/ios/build-ipa.sh"), "--check-toolchain"],
                                  cwd=ROOT, env=env, capture_output=True, text=True)

    def test_reported_mac_and_patch_update_pass_without_building(self):
        for version in ["27.1", "27.1.1"]:
            result = self.run_preflight(version, version, "/fixture/SelectedXcode")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn(f"Xcode {version}; iPhoneOS SDK {version}; deployment target 17.0", result.stdout)

    def test_wrong_xcode_or_sdk_fails_with_both_observed_versions(self):
        for xcode, sdk in [("26.4", "26.4"), ("27.1", "26.4"), ("27.2", "27.2"), ("27.10", "27.10")]:
            result = self.run_preflight(xcode, sdk)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(f"found Xcode {xcode}, SDK {sdk}", result.stderr)


if __name__ == "__main__":
    unittest.main()
