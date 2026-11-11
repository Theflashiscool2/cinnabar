"""The diagnostic run must launch fresh Cargo output without damaging old hard links."""
import contextlib
import io
import json
import os
from pathlib import Path
import re
import tempfile
import unittest
from unittest.mock import patch

SCRIPT = Path(__file__).with_name('test-java.sh').read_text()
STAGE = re.search(r"python3 - <<'PYCLIENT'\n(.*?)\nPYCLIENT", SCRIPT, re.S).group(1)


class StagingTests(unittest.TestCase):
    def stage(self, target):
        with patch('subprocess.check_output', return_value=json.dumps({'target_directory': str(target)}).encode()), contextlib.redirect_stdout(io.StringIO()):
            exec(compile(STAGE, 'stage_compiled_client', 'exec'), {})

    def test_external_target_replaces_client_but_keeps_old_bundle_inode(self):
        with tempfile.TemporaryDirectory() as directory, contextlib.chdir(directory):
            source = Path('external/play/cinnabar')
            source.parent.mkdir(parents=True)
            source.write_bytes(b'new compiled client')
            source.chmod(0o755)
            destination = Path('target/play/cinnabar')
            destination.parent.mkdir(parents=True)
            destination.write_bytes(b'old client')
            bundle = Path('previous-bundle-client')
            os.link(destination, bundle)
            self.stage(Path('external').resolve())
            self.assertEqual(destination.read_bytes(), source.read_bytes())
            self.assertEqual(bundle.read_bytes(), b'old client')
            self.assertTrue(os.access(destination, os.X_OK))
            self.assertFalse(list(destination.parent.glob('.cinnabar-stage-*')))

    def test_default_target_preserves_existing_compiled_inode(self):
        with tempfile.TemporaryDirectory() as directory, contextlib.chdir(directory):
            client = Path('target/play/cinnabar')
            client.parent.mkdir(parents=True)
            client.write_bytes(b'compiled')
            inode = client.stat().st_ino
            self.stage(Path('target').resolve())
            self.assertEqual(client.stat().st_ino, inode)

    def test_missing_cargo_artifact_fails_without_replacing_existing_client(self):
        with tempfile.TemporaryDirectory() as directory, contextlib.chdir(directory):
            client = Path('target/play/cinnabar')
            client.parent.mkdir(parents=True)
            client.write_bytes(b'old')
            with self.assertRaises(FileNotFoundError):
                self.stage(Path('missing-target').resolve())
            self.assertEqual(client.read_bytes(), b'old')
            self.assertFalse(list(client.parent.glob('.cinnabar-stage-*')))
