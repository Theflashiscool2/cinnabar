#!/usr/bin/env python3
"""Copy allowlisted diagnostic logs; never collect account/credential stores."""
import argparse
import json
import os
import re
from pathlib import Path
import shutil
import time

NAMES = ("client.log", "core.log", "java-join-latest.json")


def collect(destination, roots, since, recover_existing=False):
    destination.mkdir(parents=True, exist_ok=True)
    copied = []
    for root in dict.fromkeys(roots):
        # Retained attempts keep their original modification times after rotation.
        # Include them through the same current-run filter, never a broad JSON glob.
        retained = sorted(p.name for p in root.glob("java-join-*.json")
                          if re.fullmatch(r"java-join-[1-9][0-9]*\.json", p.name))
        for name in (*NAMES, *retained):
            source = root / name
            if not source.is_file():
                continue
            modified = source.stat().st_mtime
            current = modified >= since
            if not current and not recover_existing:
                continue
            target = destination / (str(len(copied) + 1) + "-" + name)
            while target.exists():
                target = target.with_name("next-" + target.name)
            shutil.copy2(source, target)
            copied.append({"source": str(source), "file": target.name, "modified": modified,
                           "provenance": "current_run" if current else "pre_existing_not_verified_as_this_run"})
    manifest = destination / ("collected-" + str(time.time_ns()) + ".json")
    manifest.write_text(json.dumps({"since": since, "logs": copied}, indent=2) + "\n")
    return copied


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path, help="the test folder containing console.log")
    parser.add_argument("--recover-existing", action="store_true",
                        help="also copy older allowlisted logs, explicitly labelled as pre-existing")
    args = parser.parse_args()
    console = args.destination / "console.log"
    if not console.is_file() and not args.recover_existing:
        parser.error("console.log is missing; use the test folder created before launching")
    # Console starts before launch and receives append writes. Its final mtime is
    # not the start time; the explicit marker keeps short/early errors eligible.
    marker = args.destination / "started"
    if not marker.is_file() and not args.recover_existing:
        parser.error("started marker is missing; refusing to collect unrelated older logs")
    roots = [Path.cwd() / ".local/logs"]
    if os.environ.get("CINNABAR_USER_ROOT"):
        roots.append(Path(os.environ["CINNABAR_USER_ROOT"]) / "data/logs")
    support = Path.home() / "Library/Application Support"
    roots.extend(p / "data/logs" for p in support.glob("Cinnabar*"))
    roots.extend(p / "data/logs" for p in support.glob("app.cinnabar*"))
    since = marker.stat().st_mtime if marker.is_file() else time.time()
    copied = collect(args.destination, roots, since, args.recover_existing)
    print("Collected", len(copied), "logs in", args.destination)
    if any(p["provenance"] != "current_run" for p in copied):
        print("Older logs are included and labelled; they do not establish which app/build produced them.")
    if not any(p["file"].endswith("java-join-latest.json") for p in copied):
        print("No current join report found. Disconnect in the app to flush it, then collect again.")
    print("Keep console.log and add the exact chat rejection/screenshot and the failed action.")


if __name__ == "__main__":
    main()
