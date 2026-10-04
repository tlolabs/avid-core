# AVID Core FFmpeg runtime

AVID Core is the single source of truth for the FFmpeg runtime used by ATIV,
EnCAP and other TLO Labs applications. It builds `ffmpeg` and `ffprobe` together
from pinned FFmpeg 9.0.1 source, Recipe 7, and packages them with source/build
provenance, licenses, validation, repeat-build evidence and checksums. Consumers
use `MediaTools::from_core_directory` with a complete acquired Core package;
they do not choose their own FFmpeg version or pair it with a system FFprobe.

The current archive layout is one target-specific directory containing
`ffmpeg`/`ffprobe` (or `.exe` on Windows), `spec.json`, `build.json`,
`source-provenance.json`, `validation.json`, `repeat-build.json`, `SOURCE.json`,
`core-tests-passed.txt`, `SHA256SUMS` and `licenses/`. The sibling sources
archive contains the corresponding source and build scripts. Core validates
one directory and exposes both paths and their build identity; it never falls
back to PATH in this API. Internal checksums detect corruption. Acquisition
must verify the archive's external authenticity before production use.

Run a native build with `bash scripts/ffmpeg/ci.sh <target>` from a clean
checkout. See `scripts/ffmpeg/README.md` for target IDs and local prerequisites.
The manual native CI matrix performs this build for macOS, Windows and Linux on
arm64 and x86_64, uploads candidate archives and source, and publishes no
production release. A target is qualified only after its actual native and
release gates in `runtime/ffmpeg/qualification.json` pass. Historical reports
under `docs/ffmpeg/historical/` describe the earlier distribution design.
