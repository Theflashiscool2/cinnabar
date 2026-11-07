#!/usr/bin/env python3
"""Copy current-run diagnostic logs only; never collect account/credential stores."""
import argparse
import json
import os
from pathlib import Path
import shutil
import time

NAMES = ("client.log", "core.log", "java-join-latest.json")


def collect(destination, roots, since):
    destination.mkdir(parents=True, exist_ok=True)
    copied = []
    for root in dict.fromkeys(roots):
        for name in NAMES:
            source = root / name
            if not source.is_file() or source.stat().st_mtime < since:
                continue
            target = destination / (str(len(copied) + 1) + "-" + name)
            while target.exists():
                target = target.with_name("next-" + target.name)
            shutil.copy2(source, target)
            copied.append({"source": str(source), "file": target.name})
    manifest = destination / ("collected-" + str(time.time_ns()) + ".json")
    manifest.write_text(json.dumps({"since": since, "logs": copied}, indent=2) + "\n")
    return copied


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path, help="the test folder containing console.log")
    args = parser.parse_args()
    console = args.destination / "console.log"
    if not console.is_file():
        parser.error("console.log is missing; use the test folder created before launching")
    # Console starts before launch and receives append writes. Its final mtime is
    # not the start time; the explicit marker keeps short/early errors eligible.
    marker = args.destination / "started"
    if not marker.is_file():
        parser.error("started marker is missing; refusing to collect unrelated older logs")
    roots = [Path.cwd() / ".local/logs"]
    if os.environ.get("CINNABAR_USER_ROOT"):
        roots.append(Path(os.environ["CINNABAR_USER_ROOT"]) / "data/logs")
    support = Path.home() / "Library/Application Support"
    roots.extend(p / "data/logs" for p in support.glob("Cinnabar*"))
    roots.extend(p / "data/logs" for p in support.glob("app.cinnabar*"))
    copied = collect(args.destination, roots, marker.stat().st_mtime)
    print("Collected", len(copied), "current-run logs in", args.destination)
    if not any(p["file"].endswith("java-join-latest.json") for p in copied):
        print("No current join report found. Disconnect in the app to flush it, then collect again.")
    print("Keep console.log and add the exact chat rejection/screenshot and the failed action.")


if __name__ == "__main__":
    main()
