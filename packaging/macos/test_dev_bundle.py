"""Exercise development bundle assembly without a compiler or Apple runtime."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]


class DevBundleTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for name in ("Makefile", "packaging/macos/Info.plist.in",
                     "packaging/macos/build-app.sh", "assets/vanilla-source.json",
                     "packaging/common/runtime-binaries.txt"):
            dest = self.root / name
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / name, dest)
        self.commands = self.root / "commands"
        self.commands.mkdir()
        self.command("uname", "printf 'Darwin\\n'")
        # A buildless launch must never invoke a compiler or asset preparation.
        self.command("cargo", "exit 99")
        self.command("go", "exit 99")
        self.binaries = self.root / "target/play"
        self.binaries.mkdir(parents=True)
        for name in ("cinnabar", "bedrock-core"):
            path = self.binaries / name
            path.write_text("#!/bin/sh\nprintf launched > launched.txt\n")
            path.chmod(0o755)
        self.bundle = self.binaries / "Cinnabar Dev.app/Contents/MacOS"

    def command(self, name, body):
        path = self.commands / name
        path.write_text("#!/bin/sh\n" + body + "\n")
        path.chmod(0o755)

    def launch(self):
        return subprocess.run(
            ["make", "play-run", "CINNABAR_BUILD_COMMIT=fixture",
             "PKG_VERSION=0.1.31", "DEV_APP_ID=app.cinnabar.test"],
            cwd=self.root, text=True, capture_output=True,
            env={**os.environ, "PATH": str(self.commands) + os.pathsep + os.environ["PATH"]},
        )

    def test_hard_links_and_launch(self):
        result = self.launch()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((self.root / "launched.txt").is_file())
        self.assertTrue(os.path.samefile(self.binaries / "cinnabar", self.bundle / "cinnabar"))

    def test_copy_fallback_replaces_stale_binaries_and_cleans_absent_helpers(self):
        self.command("ln", "exit 1")
        self.bundle.mkdir(parents=True)
        (self.bundle / "cinnabar").write_text("stale")
        (self.bundle / "mod-host").write_text("stale helper")
        result = self.launch()
        self.assertEqual(result.returncode, 0, result.stderr)
        for name in ("cinnabar", "bedrock-core"):
            self.assertEqual((self.binaries / name).read_bytes(), (self.bundle / name).read_bytes())
            self.assertFalse(os.path.samefile(self.binaries / name, self.bundle / name))
            self.assertTrue(os.access(self.bundle / name, os.X_OK))
        self.assertFalse((self.bundle / "mod-host").exists())
        self.assertTrue((self.root / "launched.txt").is_file())

    def test_failed_copy_stops_before_launch(self):
        self.command("ln", "exit 1")
        self.command("cp", "exit 1")
        result = self.launch()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / "launched.txt").exists())


if __name__ == "__main__":
    unittest.main()
