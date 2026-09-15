# Host migration handoff: ATIV, then EnCAP

## Current gate

**Do not change host acquisition or remove working binaries yet.** The AVID Core implementation is established and a macOS ARM64 candidate is verified locally. The source manifest remains `candidate` and blocks publication because the full CI/toolchain/hardware/host matrix is unfinished. Hosts have unrelated uncommitted work; preserve it. This document supersedes the FFmpeg ownership paragraphs in the earlier extraction handoffs.

## Core owner: finish qualification first

1. Review this specification, exact upstream provenance and licensing, including PGP/tag-tree verification. Implement and pin Linux/Windows hardware dependencies demonstrated by existing target binaries; their current software-only candidates are not replacements.
2. Run `.github/workflows/ffmpeg.yml` from the Core repository on all six targets. Diagnose failures rather than removing targets or required capabilities. Pin/archive the qualified compiler/SDK/runner environments and compare repeat builds.
3. Run existing Core tests plus capability, parser, linkage and smoke validation. Perform actual-device tests for each currently usable hardware encoder and EnCAP AudioToolbox. Check minimum host OSes and cold execution without development libraries/PATH media tools.
4. Review complete binary/source/checksum/notice packages and attestation availability. Resolve every manifest blocker with recorded evidence. Mark the final spec qualified, rebuild that exact spec and publish the complete immutable runtime release. This implementation has not published one.

## ATIV migration: explicit changes after that gate

- Update the tested `tlolabs/avid-core` revision in `.github/workflows/native-release.yml` and the local dependency arrangement together.
- Replace `script/fetch_ffmpeg.sh`'s provider/version/asset table with a consumer of the **checked-out Core specification**, or a Core-owned acquisition utility. Inputs are only supported target and destination; callers do not supply an FFmpeg version. Obtain the matching Core release's package, verify its authenticated SHA-256/attestation, unpack the complete pair and preserve manifests/notices/source link.
- Keep existing packaging inputs `FFMPEG_BIN`, `FFPROBE_BIN`, Linux ffmpeg-directory argument and Windows `FfmpegDirectory`, now populated from the verified artifact. Change hard-coded third-party version-prefix checks in `package_macos.sh`, `package_linux.sh`, `package_windows.ps1` and release tests to the Core contract. Stable source builds report the exact release version, not BtbN's `n...-29-g...` string.
- Bundle `spec.json`/`build.json` with the pair, and the actual `licenses/`, source link and validation provenance. `FFMPEG_LICENSE_FILE` must no longer default to the host's license as FFmpeg's license.
- In production resolve the host-owned bundled directory with `MediaTools::from_managed_directory`. Preserve deliberate CLI development overrides as an explicit separate policy; no release PATH fallback. Retain software encoding defaults and the single shared pair.
- Verify original checksums before macOS/Windows signing. Keep existing host sign/notarize/staple/installer/Sparkle steps and record hashes of signed payloads separately. Never move signing credentials to Core.
- Run ATIV Rust/native/engine protocol tests, `script/test_engine_integration.sh`, `script/verify_ffmpeg_distribution.py`, package validation, preview/export comparisons and every platform's installer/startup checks with the **packaged** pair. Ensure missing/damaged bundled tools fail safely without a system installation.
- Record Core revision, runtime recipe/target/digest, signed-package identity and comparison evidence. Only after all required ATIV targets pass is its obsolete binary downloader removable.

## EnCAP migration: after ATIV evidence

- Update the tested Core revision consistently; the current host workflow pin is older than ATIV's and must be reviewed with existing host changes.
- Replace provider logic in `fetch_ffmpeg.sh` and Windows workflow's duplicated BtbN download block with the same Core mapping/verified artifact acquisition. Keep `build_ffmpeg.sh`, `build_ffmpeg_linux.sh` and `prepare_ffmpeg.sh` as host compatibility entrypoints if useful, but they must stop owning version/recipe decisions. Cache key must include Core spec/recipe/target and verified package digest, not just host script text.
- Install one matched executable pair for **Audio, Transcript and Video**. EnCAP Audio/Transcript processing need not be redesigned in this infrastructure phase; both retained wrappers and Video adapter must resolve the same managed paths. Do not add a second Video-only FFmpeg copy.
- Preserve intentional `ENCAP_FFMPEG`/`ENCAP_FFPROBE` development settings. Production uses the managed bundle and must not silently fall through to PATH. Apply Core compatibility validation once through the existing discovery-with-validator boundary, or adapt all modes to the same managed pair; do not duplicate the capability specification in encap-ffmpeg.
- Keep host package layout, ad-hoc/developer signing policy, any later notarization, update/installers and application publishing in EnCAP. Copy actual notices/manifests and maintain corresponding-source availability.
- Test MP3 metadata/ID3v2, M4A native AAC and macOS AudioToolbox AAC, attached artwork, chapters/links, WAV PCM/float and AIFF/AIFC sources, concat/channel normalization, transcript mono16k preparation, Video H.264/HEVC/automatic/hardware/software, previews, timeline ordering and cancellation. Preserve current failure cases (e.g. unsupported per-chapter artwork).
- Run Core media tests, `encap-engine --test media_contract -- --ignored` with explicit packaged paths, retained mode tests, native playback/project/save tests and all package/startup checks. Test Windows ARM64 natively; its former cross-build checks were incomplete. Record actual driver/device results, not just encoder listing.
- Only after all distributed EnCAP targets/modes pass may obsolete binary download/build cache mechanisms be retired.

## Artifact acquisition and trust

The selected Core revision determines release version/recipe and target basename. The future published tag is `ffmpeg-<version>-r<recipe>`. Use a trusted release asset digest and GitHub attestation tied to Core's repository/workflow/source revision; an unauthenticated checksum downloaded with arbitrary binaries is not a trust root. Runtime API checks do not replace artifact authentication. Keep original verification evidence through host re-signing.

Parallel candidate availability is temporary test infrastructure. Never replace existing host caches/bundles during Core validation; never ship two mode-specific pairs as the final migration. If candidate behavior fails, keep the working host release and fix the Core recipe, incrementing its recipe/mapping as appropriate.
