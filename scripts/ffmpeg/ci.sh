#!/usr/bin/env bash
# Manual diagnostic build only. No Core runtime distribution or release gate.
set -euo pipefail
TARGET="${1:?target}"
PYTHON="${AVID_BUILD_PYTHON:-python3}"
# Upstream configure paths must not contain spaces. Clean runners use this stable path.
WORK="${AVID_BUILD_ROOT:-/tmp/avid-ffmpeg-ci}"
TOOL_DIR="$WORK-tools"
TOOL_BIN="$($PYTHON scripts/ffmpeg/tools.py --target "$TARGET" --destination "$TOOL_DIR" | tail -n 1)"
export PATH="$TOOL_BIN:$PATH"
$PYTHON -m unittest discover -s scripts/ffmpeg -p 'test_*.py'
$PYTHON scripts/ffmpeg/build.py --target "$TARGET" --work "$WORK"
NAME="$($PYTHON -c 'import json,sys;sys.path.insert(0,"scripts/ffmpeg");from build import artifact_name;print(artifact_name(json.load(open("runtime/ffmpeg/spec.json")),sys.argv[1]))' "$TARGET")"
RUNTIME="$PWD/dist/$NAME"
$PYTHON scripts/ffmpeg/validate.py --target "$TARGET" --directory "$RUNTIME" --report "$RUNTIME/validation.json"
# Optional VideoToolbox observation is separate from required software validation.
$PYTHON scripts/ffmpeg/hardware_probe.py --target "$TARGET" --directory "$RUNTIME" --report "$RUNTIME/hardware-observation.json"
export PATH="$RUNTIME:$PATH"
export AVID_RUNTIME_DIRECTORY="$RUNTIME"
if [[ "$TARGET" == windows-* ]]; then export AVID_RUNTIME_DIRECTORY="$(cygpath -w "$RUNTIME")"; fi
cargo test --locked --all-targets -- --test-threads=1
cargo test --locked --test ffmpeg -- --ignored --test-threads=1
cargo test --locked --test runtime_contract -- --ignored --test-threads=1
# Optional repeat-build research. No archive promotion or host qualification.
$PYTHON scripts/ffmpeg/repeat.py --target "$TARGET" --work "$WORK" --first "$RUNTIME"
