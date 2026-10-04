# Core native FFmpeg build

Recipe 7 builds FFmpeg 9.0.1 and FFprobe from the same verified source tree.
`build.py` fetches source archives and immutable source revisions only, checks
the official FFmpeg release and signed tag, builds pinned dependencies and both
programs, and records source/build identity, flags, toolchain, notices and
corresponding source. `validate.py` checks both executables, architecture,
versions, matching configuration, linkage, capabilities and real media probes.
`repeat.py` compares both executables from two clean source builds.
`package.py` accepts only a clean candidate with complete validation, repeat
build, Core tests, notices and sources. It writes internal and archive SHA-256
checksums. A candidate archive is not a release.

On a native target, run `bash scripts/ffmpeg/ci.sh <target>` from a clean
checkout. The script obtains manifest-pinned CMake, runs Python and Rust tests,
builds, validates, repeats the source build, packages, and tests the Core
loader against the resulting archive directory. Its default build root is
`/tmp/avid-ffmpeg-ci`; set `AVID_BUILD_ROOT` to a fresh path without spaces if
needed. Cached source archives are under `.ffmpeg-work/downloads`.

Supported target IDs are `macos-arm64`, `macos-x86_64`, `windows-x86_64`,
`windows-arm64`, `linux-x86_64` and `linux-arm64`. The manual `ffmpeg.yml`
workflow uses their native runners and uploads candidates, never a release.
See `docs/ffmpeg/core-runtime-architecture.md` for qualification limits.
