# Local implementation verification

> Scope update (2026-09-15, recipe 6): software encoding is authoritative. Windows/Linux GPU interfaces and parity are not required. Historical hardware requirements below are superseded by [the current policy](README.md#configuration-and-dependencies). Minimum-OS qualification remains required. Results below describe earlier recipes unless explicitly identified otherwise.

## Result

The complete `scripts/ffmpeg/ci.sh macos-arm64` entrypoint successfully downloaded verified official build tools/sources, compiled all required source libraries and FFmpeg/FFprobe, validated the pair, ran Core compatibility tests and produced runtime/source archives with SHA-256 files. No host files, working binaries or acquisition mechanisms were changed. No remote CI run or runtime release was published.

Two clean macOS ARM64 builds at the same absolute build prefix produced **byte-identical ffmpeg and ffprobe executables**. [The recorded digests](repeat-build.json) are reproducibility evidence for this local environment only. It does not establish reproducibility on refreshed runners, other toolchains or other platforms. Script changes between these runs concerned Windows paths/build handling and did not change macOS compilation; configuration and compiler inputs matched.

## Checks run

| Check | Result |
| --- | --- |
| Full native source build and package entrypoint | Passed on macOS ARM64 |
| Source SHA-256 / x264 immutable Git revision checks | Passed |
| Actual runtime version and mandatory capabilities | Passed |
| Generated parser configuration | Passed |
| macOS dynamic linkage | System libraries/frameworks only |
| Known-working ATIV pair: smoke suite | Passed; current EnCAP ARM64 pair has identical hashes |
| Source-built candidate: same smoke suite | Passed; result assertions match baseline |
| Default Core tests | 41 passed, including managed-runtime negative tests |
| Existing Core real-media tests | All 5 passed |
| Strict managed-runtime integration | Passed |
| Infrastructure negative tests | 5 passed: capability parsing, matrix preservation, baseline rejection, unqualified/incomplete/checksum-invalid release rejection |
| Rust formatting, all-target check and clippy with warnings denied | Passed |
| Minimum supported Rust 1.85 all-target check | Passed |
| Compiled README documentation example | Passed |
| Workflow YAML and actionlint 1.7.12 | Passed |
| Clean repeat executable comparison | Both identical |

Smoke tests cover integer/float WAV and AIFF, FLAC, MP3, AAC, ALAC/Vorbis/Opus/WMA fixture decoding, PNG/JPEG/BMP/TIFF/WebP artwork, image sizing/scaling, M4A/MP3 metadata/chapters/cover art, macOS AudioToolbox AAC, audio concat, mono16k transcript PCM, H.264/HEVC MP4, hvc1, cadence and progress. Core tests add composition pixels, preview, flips, cached-frame/legacy comparison, original audio format, timeline hard cuts, fractional duration, cancellation/timeouts and software fallback.

The sources are tiny synthetic fixtures generated with the existing trusted binary and recorded commands/digests; they are not external media assets. Four initially passing media tests plus a missing testsrc2 filter exposed by the fifth informed the final candidate configuration. Baseline validation also established that lavfi must be checked through `-devices` and that FFmpeg has no `-parsers` CLI query.

## Environment and limitations

Apple ARM64, Apple clang 21.0.0 (`clang-2100.0.123.102`), SDK 26.4.1, macOS deployment target 13.0, official CMake 3.31.6. Exact configure flags, tool versions, source revision and binary hashes are in packaged `build.json`/`validation.json`; the manifest is authoritative for source/dependency pins. Local candidate metadata identifies the base Core commit and a modified working tree; it is not an attested clean CI commit. Production releases must originate from the clean, fully qualified Core CI workflow.

Not verified here: native Intel/Windows/Linux builds, complete CI artifact aggregation/attestation/publication, Linux/Windows hardware SDK/source recipes, actual GPU parity, minimum-OS behavior, official PGP signature and archive/tag-tree equivalence, final host packaging/signing/notarization, or host migration integration. These are explicit manifest/documented gates. In particular, a software-only Linux/Windows candidate must not displace a working build that advertises/uses hardware encoders. The working hosts remain authoritative for shipped behavior until migration passes.
