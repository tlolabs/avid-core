# AVID Core FFmpeg runtime

AVID Core is the single source of truth for the FFmpeg runtime used by ATIV,
EnCAP and other TLO Labs applications. It builds `ffmpeg` and `ffprobe` together
from pinned FFmpeg 9.0.1 source, Recipe 7, and packages them with source/build
provenance, licenses, validation, repeat-build evidence and checksums. Consumers
select an appropriate published Core runtime and use
`MediaTools::from_core_directory` with its complete acquired package; they do
not assemble their own FFmpeg/FFprobe pair or pair it with a system FFprobe.

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
The [independent platform workflow](../../.github/workflows/ffmpeg-independent.yml)
runs after relevant changes reach `main` and can also be dispatched. Each macOS,
Windows, and Linux target builds, verifies, and publishes its own immutable
direct-download Release as soon as its checks pass. Another target may still be
queued or fail. The [legacy candidate workflow](../../.github/workflows/ffmpeg.yml)
remains available for the pinned authenticated release plan. Authenticated
runtime promotion still requires the complete six-target manifest because current
consumers verify it as one update. Host packaging and signing evidence are
informational, not Core release prerequisites. See [the provider contract](provider-contract.md)
for consumer responsibilities and migration. Historical reports under
`docs/ffmpeg/historical/` describe the earlier distribution design.
