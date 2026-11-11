#!/bin/bash
# Cached native Java diagnostic run; existing prepared assets/companions are reused.
set -euo pipefail
cd "$(dirname "$0")/../.."
if [ "$(uname -s)" != Darwin ]; then
  echo 'Run this helper on the Mac that already has the prepared Cinnabar assets.' >&2
  exit 1
fi
# Never create a replacement directory beneath an absent external-volume mount.
for cache_path in "${CARGO_BUILD_BUILD_DIR:-}" "${CARGO_TARGET_DIR:-}" "${GOCACHE:-}"; do
  case "$cache_path" in
    /Volumes/*)
      volume_name=${cache_path#/Volumes/}
      volume_name=${volume_name%%/*}
      if ! mount | grep -F " on /Volumes/$volume_name (" >/dev/null; then
        echo "STOP: /Volumes/$volume_name is not mounted." >&2
        exit 1
      fi
      ;;
  esac
done
python3 - "$PWD" "${CARGO_BUILD_BUILD_DIR:-}" "${CARGO_TARGET_DIR:-}" <<'PYSPACE'
from pathlib import Path
import shutil,sys
for value in filter(None,sys.argv[1:]):
    path=Path(value).absolute()
    while not path.exists():path=path.parent
    free=shutil.disk_usage(path).free
    if free<6*1024**3:
        raise SystemExit(f"STOP: only {free/1024**3:.1f} GiB free at {path}; need at least 6 GiB before building.")
PYSPACE
export CINNABAR_JAVA_ASSETS="${CINNABAR_JAVA_ASSETS:-$PWD/.local/assets/java/26.3}"
for asset in client.jar blocks.json models.json physics.bin interaction.bin; do
  if [ ! -f "$CINNABAR_JAVA_ASSETS/$asset" ]; then
    echo "STOP: missing existing Java input $CINNABAR_JAVA_ASSETS/$asset" >&2
    exit 1
  fi
done
if [ ! -f target/play/bedrock-core ]; then
  echo 'STOP: companion binaries are missing. Run make play-build once, then retry.' >&2
  exit 1
fi
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CINNABAR_JAVA_TELEMETRY=1
export CINNABAR_SLOW_FRAME_LOG=0
export CINNABAR_LOG='warn,java_session=debug,java_auth=info,client_shell::menu=info'
unset CINNABAR_JAVA_OFFLINE_FIXTURE
export CINNABAR_BUILD_COMMIT
CINNABAR_BUILD_COMMIT=$(git rev-parse HEAD)
JAVA_TEST_DIR=$(mktemp -d "$HOME/Desktop/cinnabar-hypixel.XXXXXX")
touch "$JAVA_TEST_DIR/started"
printf '%s\n' "$CINNABAR_BUILD_COMMIT" > "$JAVA_TEST_DIR/source-commit.txt"
printf '%s\n' "$JAVA_TEST_DIR" > "$HOME/Desktop/cinnabar-last-test.txt"
echo "Build and run logs: $JAVA_TEST_DIR"
set +e
{
  cargo build --profile play --locked -p cinnabar && make PROFILE=play play-run
} 2>&1 | tee "$JAVA_TEST_DIR/console.log"
run_status=${PIPESTATUS[0]}
set -e
python3 packaging/macos/collect-java-diagnostics.py "$JAVA_TEST_DIR"
ditto -c -k --keepParent "$JAVA_TEST_DIR" "$JAVA_TEST_DIR.zip"
echo "Send this archive: $JAVA_TEST_DIR.zip"
exit "$run_status"
