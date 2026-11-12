"""Exercise log collection without accessing real accounts or platform directories."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("collector", Path(__file__).with_name("collect-java-diagnostics.py"))
collector = importlib.util.module_from_spec(spec)
spec.loader.exec_module(collector)


class CollectionTests(unittest.TestCase):
    def test_current_logs_only_and_credentials_never_collected(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            logs = root / "logs"
            logs.mkdir()
            for name in [*collector.NAMES, "java-accounts.json", "credentials.json", "token-cache"]:
                (logs / name).write_text("fixture")
            import os
            os.utime(logs / "java-join-latest.json", (1, 1))
            output = root / "out"
            result = collector.collect(output, [logs, logs], 2)
            self.assertEqual([Path(p["source"]).name for p in result], ["client.log", "core.log"])
            self.assertFalse(any("credentials" in p.name or "accounts" in p.name for p in output.iterdir()))
            original = (output / "1-client.log").read_bytes()
            collector.collect(output, [logs], 2)
            self.assertEqual((output / "1-client.log").read_bytes(), original)
            self.assertTrue((output / "next-1-client.log").is_file())

    def test_recovery_labels_older_logs_without_collecting_credentials(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "client.log").write_text("older app still running")
            (root / "java-accounts.json").write_text("not a diagnostic")
            result = collector.collect(root / "out", [root], float("inf"), recover_existing=True)
            self.assertEqual(len(result), 1)
            self.assertEqual(result[0]["provenance"], "pre_existing_not_verified_as_this_run")
            self.assertEqual(Path(result[0]["source"]).name, "client.log")

    def test_rotated_attempts_keep_current_run_filter_and_exact_filename_allowlist(self):
        import os
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            logs = root / "logs"
            logs.mkdir()
            for name in ["java-join-latest.json", "java-join-1.json", "java-join-2.json",
                         "java-join-credentials.json", "java-join-1.json.secret",
                         "java-join-1-auth.json", "java-join-0.json"]:
                (logs / name).write_text("{}")
                os.utime(logs / name, (3, 3))
            os.utime(logs / "java-join-2.json", (1, 1))
            result = collector.collect(root / "out", [logs], 2)
            self.assertEqual([Path(p["source"]).name for p in result],
                             ["java-join-latest.json", "java-join-1.json"])
            result = collector.collect(root / "recovered", [logs], 2, recover_existing=True)
            older = [p for p in result if Path(p["source"]).name == "java-join-2.json"]
            self.assertEqual(len(older), 1)
            self.assertEqual(older[0]["provenance"], "pre_existing_not_verified_as_this_run")


if __name__ == "__main__":
    unittest.main()
