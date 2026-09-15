# AVID-owned FFmpeg runtime

AVID Core owns the FFmpeg source, dependency versions, build recipe, compatibility contract and runtime artifact mapping for ATIV and every EnCAP mode. Hosts own installation layout, application signing, notarization, installers and final application releases.

**Implementation status: candidate, not approved for host replacement.** A macOS ARM64 runtime has been built twice with byte-identical FFmpeg/FFprobe executables at the same absolute build path, and passed the baseline smoke suite, all five existing Core real-media tests and managed-runtime API validation. The other five targets have CI recipes but have not run here. Linux/Windows hardware parity and toolchain qualification remain release blockers. The manifest deliberately prevents publication until these are resolved. Neither host repository has been modified.

## Read first

- [Local verification report](verification.md).
- [Current architecture and capability audit](audit.md), including recorded executable inventories.
- [Licensing and redistribution](licensing.md).
- [Host migration instructions and acceptance gates](migration.md).
- `runtime/ffmpeg/spec.json`: the **only authoritative machine-readable release/version mapping**. Read `source.version`, `source.revision`, `recipe`, `targets`, `status` and `qualification_blockers`; do not parse this document for versions.

## Source and recipe identity

The initial candidate uses the official stable FFmpeg 9.0.1 release, tag `n9.0.1`, commit `bf1b838f2ab88b4f8fd83443325c782ea0e0f7fa`. Its official source archive and SHA-256 are in the specification. Official upstream locations: [release downloads](https://ffmpeg.org/download.html) and [Git repository](https://git.ffmpeg.org/ffmpeg.git). No FFmpeg executable is fetched by this build.

Archive SHA-256 verification is mandatory, including on a cache hit. FFmpeg's `VERSION` is checked after extraction. x264 is fetched from its official repository at an immutable stable-branch commit and `FETCH_HEAD` must equal that commit. Its Git archive is included with corresponding source. All other library sources use checksum-pinned upstream release archives. No moving FFmpeg branch, binary provider, package-manager FFmpeg or system FFmpeg supplies a release artifact.

The official FFmpeg release signing-key fingerprint is recorded. The initial archive was acquired over verified HTTPS and its tag's peeled revision read from official Git. **PGP signature verification has not been performed locally**; repeat verification with the recorded fingerprint when qualifying a release. A digest records the selected bytes; it is not independently a signature or proof that archive and tag trees match. Verify their equivalence as part of provenance qualification, following upstream's download instructions.

The recipe version is independent of the Rust crate version. A Core commit embeds the exact specification with `include_str!`. A Core version therefore maps to one source/recipe/target set even when its Rust semantic version does not change with FFmpeg's numbering. Increment `recipe` for any build/dependency/toolchain change, and release a new Core version/revision when changing its expected runtime. Existing releases are immutable: never replace their assets.

## Target matrix

| Runtime target | Native CI runner | Host distribution evidence | Status |
| --- | --- | --- | --- |
| macos-arm64 | macos-15 | ATIV and EnCAP | Local source build + compatibility passed; CI/oldest OS/signing still pending |
| macos-x86_64 | macos-15-intel | ATIV and EnCAP | CI recipe, not yet executed |
| windows-x86_64 | windows-2025 | ATIV and EnCAP | Candidate recipe; hardware and toolchain qualification blocked |
| windows-arm64 | windows-11-arm | ATIV and EnCAP | Candidate recipe; hardware and toolchain qualification blocked |
| linux-x86_64 | ubuntu-24.04 | ATIV and EnCAP | Candidate recipe; hardware and toolchain qualification blocked |
| linux-arm64 | ubuntu-24.04-arm | ATIV | Candidate recipe; hardware and toolchain qualification blocked |

EnCAP currently distributes Linux x64 only; its fetch script also accepts ARM64. ATIV establishes the union's Linux ARM64 requirement. EnCAP's Windows ARM64 packaging previously cross-built on x64 and skipped some native media execution. Core CI uses the native Windows ARM64 runner to close that gap. No target is silently removed.

macOS deployment target is 13.0, matching the hosts. Apple SDK frameworks are OS dependencies, not downloaded codec libraries. Linux uses the Ubuntu 24.04 compiler/libc environment and records dynamic linkage; verify the oldest supported host OS before migration. Windows uses MSYS2 CLANG64/CLANGARM64, static third-party libraries and Windows system DLLs; it must not need MSYS2 at runtime. Windows setup and hardware dependencies need qualification before any release claim.

## Configuration and dependencies

`spec.json` enumerates common flags, enabled encoders/filters, mandatory capabilities and per-target requirements. `build.py` holds dependency build commands and explicit platform additions; both are version-controlled and hashed into build metadata. Its important policies are:

- `--disable-autodetect`: runner-installed codec libraries cannot silently enter the build.
- Static FFmpeg and codec libraries, both `ffmpeg` and `ffprobe`, no `ffplay`, no network protocols, only `file,pipe`.
- Existing software encoders: libx264, libx265, libmp3lame, native AAC and PCM; image/fixture encoders support validation and previews.
- Existing graph filters, implicit audio resampling, and small synthetic test sources. FFmpeg's lavfi is an **input device**, not a demuxer listing entry.
- Retain native demuxers/decoders/parsers/bitstream filters for the hosts' broad file acceptance. No new external codecs are introduced. A strict decoder allowlist would prematurely restrict `.image`, MIME audio and CLI path input. Further narrowing needs a tested input corpus and explicit compatibility decision.
- Enable GPL; explicitly disable version3 and nonfree. See licensing review.

External source dependencies: x264 (existing H.264 software), x265 (existing HEVC software), LAME (existing MP3 export), zlib (PNG artwork). NASM is a pinned source-built **build tool** for x86 assembly; it is not a codec/runtime dependency. x264 and x265 assembly remain enabled. Native FFmpeg handles AAC, ALAC, FLAC, Vorbis, Opus, WMA and image decoding without importing unrelated third-party encoders. CMake is a pinned, checksum-verified upstream build tool; it is never included as a media runtime.

The candidate enables existing macOS VideoToolbox and AudioToolbox interfaces. The code's NVENC/QSV/AMF/VAAPI choices are unchanged. **The candidate Linux/Windows profile does not yet implement their external SDK/header/library recipes. It must not replace existing host builds.** Before qualification, inventory the existing target binaries, pin only the necessary authoritative dependencies (e.g. nv-codec-headers, AMF headers, oneVPL, libva/libdrm as demonstrated), build them from source, record licenses, include any required non-system runtime libraries, enforce their advertised capabilities and validate on real devices. Do not remove those capabilities or change encoder selection to make the gate pass.

## Local build and validation

Prerequisites: Python 3.12+, Git, curl, GNU Make, pkg-config/pkgconf and the target's native C/C++ compiler. The manifest defines CMake 3.31.6; newer CMake 4 rejects x265's old policies. No global package installation is required for CMake:

```sh
# Run inside the AVID Core checkout. Work directories must be fresh and have no spaces.
python3 scripts/ffmpeg/tools.py --target macos-arm64 --destination /tmp/avid-cmake
# Add the printed CMake bin directory to PATH.
python3 scripts/ffmpeg/build.py --target macos-arm64 --work /tmp/avid-ffmpeg-build
# Read the package name from build output or managed_runtime_artifact_name(target).
python3 scripts/ffmpeg/validate.py --target macos-arm64 \
  --directory dist/avid-ffmpeg-9.0.1-r1-macos-arm64 \
  --report dist/avid-ffmpeg-9.0.1-r1-macos-arm64/validation.json
```

For the complete build/test/package sequence use `bash scripts/ffmpeg/ci.sh macos-arm64`; substitute another supported native target. This script acquires pinned CMake, builds the runtime, runs capability/linkage/media validation, default Core tests, all five real-media tests and managed API validation, and only then writes the package checksums. In MSYS2 use `/usr/bin/python3` as `AVID_BUILD_PYTHON`, the appropriate native compiler shell and the native Rust toolchain on PATH.

`--baseline` on `validate.py` runs the same behavioral checks against existing trusted tools without asserting canonical provenance. Such reports are explicitly marked baseline and rejected by the packager. It is for comparison only.

## Reproducibility and caching

Inputs are pinned; builds use fresh source/build directories. Downloaded source/Git objects are cached with specification + scripts + target keys and always verified. Compiled outputs and dependency installations are not cached. Thus the current compiler is never paired accidentally with a dependency cache from another compiler. The tools installer also verifies cached CMake archives.

The build clears inherited compiler/include/library/pkg-config overrides; `PKG_CONFIG_LIBDIR` points only to the private prefix. Optimization/prefix-map flags, `SOURCE_DATE_EPOCH`, `ZERO_AR_DATE`, C locale and UTC are explicit. Windows requests static linkage and suppresses the PE linker timestamp. Tar/gzip package owner/time fields are normalized. Use the same absolute build path when comparing binaries: FFmpeg embeds its configure command including the private prefix. This path requirement is documented, not hidden by claiming arbitrary-directory byte identity.

`build.json` records full configure arguments, source revision, specification digest, recipe, target, Core revision, build-script digests, compiler/tool versions, SDK, runner image, CI run and configured parsers. Package metadata legitimately differs between CI runs. Compare **executable bytes** separately from provenance-bearing archive bytes.

**Remaining reproducibility boundary:** GitHub hosted runner labels and MSYS2 build-environment packages are not immutable images. Compiler/SDK/make/pkg-config versions are recorded, not all exact-pinned/enforced yet. They are not claimed to be hermetic or byte-reproducible across runner refreshes. Qualification must lock or archive the successful per-target environment, compare repeat builds, review OS symbol/import baselines and document unavoidable linker/signature differences. No Homebrew/Chocolatey/system FFmpeg is a release runtime; system build tools and OS frameworks are a separate issue.

## Runtime API and packaging contract

`FFMPEG_RUNTIME_SPECIFICATION` exposes the embedded machine-readable mapping. `managed_runtime_artifact_name(target)` returns the package basename. `MediaTools::from_managed_directory(path, token)` requires `spec.json` and `build.json`, matching source/recipe/target, the exact stable version and all required advertised capabilities. It uses explicit executable paths with no PATH fallback, and the existing cancellable/bounded process runner. This is a compatibility check, **not cryptographic authentication**.

Keep existing `MediaTools::discover` behavior during staged migration. Host environment overrides remain adapter responsibilities; the shared core does not read ATIV/EnCAP-specific variables. A final production host must select its bundled managed directory and fail if it is missing. It may retain a deliberate development override path separately, provided a release cannot silently select an external runtime.

## Artifacts and release gates

Candidate basename: `avid-ffmpeg-<version>-r<recipe>-<os>-<arch>`.

Each `.tar.gz` contains one directory with `ffmpeg[.exe]`, `ffprobe[.exe]`, `spec.json`, `build.json`, `validation.json`, test evidence, license texts and `SHA256SUMS`. It must have no unbundled non-system runtime libraries; linkage checks reject them. A matching `-sources.tar.gz` contains exact upstream source archives/Git exports and build/validation sources. Both archives have detached SHA-256 files. Neither executable nor build tree is committed to Git.

`.github/workflows/ffmpeg.yml` derives its matrix and release tag from the specification. Every required job runs with `fail-fast: false`. Failure prevents the complete job and publication. PRs and normal pushes upload **candidates only**. Explicit publication from main additionally requires `status: qualified`, zero blockers, complete binary/source packages and checksums. A publication job attests all source/runtime archives with GitHub provenance and creates an immutable runtime release. Attestation availability depends on repository/account settings and must be tested on the first release; failure is visible. No release has been published by this implementation.

Consumers pin the tested Core revision, read its mapping, authenticate checksums/attestations from that trusted release and then package the expected target. Signing changes executable bytes: verify the original artifact first, preserve provenance, and record host-signed binary hashes separately. Never compare signed bytes to unsigned checksums and silently ignore failures.

## Controlled updates

1. Check official releases for a stable release; never automatically adopt master/nightly/RC.
2. Review upstream changes, licensing, input behavior and hardware SDK compatibility. Verify release signature and archive/tag equivalence.
3. Update the single specification's release/tag/revision/archive digest, dependency pins if necessary, recipe number and fixed source epoch. Reset status to candidate and record qualification blockers.
4. Rebuild **all six targets**, verify capabilities, generated parser config, linkage, smoke suite and Core tests. Compare against the previous approved artifact on the same fixtures; test hardware on actual devices and oldest supported OSes.
5. Review repeat-build differences, toolchain/image changes, corresponding source and notices. Archive the qualified environment/evidence.
6. Clear only resolved blockers, mark qualified, rebuild the final exact specification and publish the complete attested set.
7. Release/update Core's expected mapping, then migrate and verify ATIV, then EnCAP. Remove old acquisition recipes only after those gates pass.

A future dependency checker can read `source.version` and compare official stable releases, proposing a change without upgrading automatically.

## Troubleshooting

- **CMake policy OLD errors:** use the pinned CMake tool, not CMake 4; do not patch x265 casually.
- **Source checksum/revision mismatch:** stop. A cache hit is not grounds to bypass verification. Reacquire the same official bytes or review a new pin.
- **Build path contains spaces / directory exists:** choose a fresh no-space temporary build root. The repository itself may have spaces.
- **Missing filter/codec/device:** connect it to source-code use or a real fixture before adding it. Parser availability comes from generated `config_components.h`; FFmpeg has no `-parsers` CLI flag.
- **Unexpected dylib/DLL/so:** repair the static build or explicitly bundle/document the required runtime library; never rely on a developer's installation.
- **Hardware advertised but fails:** CPU CI proves capability presence, not device usability. Retain existing automatic software fallback and explicit-hardware errors; verify driver/device requirements in the host.
- **Publication blocked:** inspect `qualification_blockers`. They are real unfinished acceptance requirements, not a flag to bypass for convenience.
