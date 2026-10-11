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
The native CI matrix runs after relevant changes reach `main` and can also be
dispatched manually. It runs macOS, Windows and Linux targets independently,
uploads each successful candidate with diagnostics, and does not publish a
production runtime. Runtime promotion requires the complete six-target manifest
because current consumers authenticate it as one update; host packaging and
signing evidence are informational, not Core release prerequisites. Historical
reports under `docs/ffmpeg/historical/` describe the earlier distribution
design.
