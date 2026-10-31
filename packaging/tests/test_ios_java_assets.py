"""Original small fixture inputs exercise hash-checked native Java IPA injection."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("ios_assets", ROOT / "packaging/ios/add-assets.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class JavaAssetsTests(unittest.TestCase):
    def test_atomic_verified_injection_excludes_private_inputs_and_replaces_stale_assets(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            local = root / "local"
            for directory in [module.COMPILED, module.installed_pack_dir("resource_pack")]:
                folder = local / directory
                folder.mkdir(parents=True)
                (folder / "fixture").write_bytes(b"shared")
            java = root / "java"
            java.mkdir()
            manifest = {"game_version": "fixture"}
            expected = {}
            for key, filename, report_path in [
                ("client_assets", "client.jar", "client.jar"),
                ("block_registry_report", "blocks.json", "reports/blocks.json"),
                ("physics_report", "physics.bin", "physics.bin"),
                ("interaction_report", "interaction.bin", "interaction.bin"),
                ("entity_models_report", "models.json", "models.json"),
                ("fluid_render_report", "fluids.json", "fluids.json"),
            ]:
                payload = ("fixture:" + filename).encode()
                (java / filename).write_bytes(payload)
                expected[filename] = payload
                manifest[key] = {"bytes": len(payload), "sha256": hashlib.sha256(payload).hexdigest(), "path": report_path}
            (java / "credentials.json").write_text("private fixture")
            (java / "server.jar").write_bytes(b"not a runtime client input")
            metadata = root / "target.json"
            metadata.write_text(json.dumps(manifest))
            ipa, out = root / "in.ipa", root / "out.ipa"
            prefix = "Payload/Cinnabar.app/" + module.resource_dir() + "/assets/java/fixture/"
            with zipfile.ZipFile(ipa, "w") as archive:
                archive.writestr("Payload/Cinnabar.app/Cinnabar", b"executable fixture")
                archive.writestr(prefix + "stale", b"old")
            with patch.object(module, "JAVA_TARGET", metadata):
                module.add_assets(ipa, local, out, java_assets=java)
                with zipfile.ZipFile(out) as archive:
                    actual = {name.removeprefix(prefix): archive.read(name) for name in archive.namelist() if name.startswith(prefix)}
                self.assertEqual(actual, expected)
                # The normal packaging command must not silently omit prepared Java inputs.
                prepared = local / "assets/java/fixture"
                prepared.parent.mkdir(parents=True)
                java.rename(prepared)
                java = prepared
                module.add_assets(ipa, local, out, force=True)
                with zipfile.ZipFile(out) as archive:
                    actual = {name.removeprefix(prefix): archive.read(name) for name in archive.namelist() if name.startswith(prefix)}
                self.assertEqual(actual, expected)
                original = out.read_bytes()
                # Equal-length corruption catches validators that check only file size.
                (java / "physics.bin").write_bytes(b"x" * len(expected["physics.bin"]))
                with self.assertRaisesRegex(module.AssetError, "SHA-256"):
                    module.add_assets(ipa, local, out, force=True, java_assets=java)
                self.assertEqual(out.read_bytes(), original)
                (java / "physics.bin").unlink()
                with self.assertRaisesRegex(module.AssetError, "missing Java runtime input"):
                    module.add_assets(ipa, local, out, force=True)
                self.assertEqual(out.read_bytes(), original)
                self.assertEqual(list(root.glob("*.tmp")), [])


if __name__ == "__main__":
    unittest.main()
