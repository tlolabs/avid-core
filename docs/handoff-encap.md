# EnCAP migration to avid-core 0.3.0

Based on read-only inspection of EnCAP `860c408d424979aefc84a26adcfec6b3052ccd8e`
(branch `codex/encap-managed-runtime-preflight`). This Core task did not modify
EnCAP. Video already delegates to Core. Normal builds retain host FFmpeg inputs;
the opt-in managed-runtime adapter and helper-checkout requirements are obsolete.
See [the public contract](integration.md). EnCAP need not wait for ATIV migration.

1. In the root `Cargo.toml`, replace the current `eab97dd...` / `=0.2.1` dependency:

   ```toml
   avid-core = { git = "https://github.com/tlolabs/avid-core.git", tag = "v0.3.0", version = "=0.3.0" }
   ```

   Prefer `rev = "<full v0.3.0 commit>"` instead of `tag` for an explicit immutable
   source pin. Keep workspace consumers in `encap-core`, `encap-video` and
   `encap-engine`. Regenerate and commit Cargo.lock (`cargo update -p avid-core`).
   Retire `runtime/core-revision` or keep it only as documentation of the source
   pin, without an adjacent helper-checkout requirement.

2. In `crates/encap-ffmpeg/src/lib.rs::MediaTools::discover_with_validator`, replace
   the `#[cfg(feature = "managed-runtime")]` block calling
   `avid_core::MediaTools::from_managed_layout`. Resolve the executable paths using
   EnCAP's chosen packaging layout and `ENCAP_FFMPEG` / `ENCAP_FFPROBE` override
   policy. Production should use one host-owned pair for Audio, Video and
   Transcript media calls. Preserve separate Audio/Transcript process APIs.
   Core does not dictate where EnCAP places executable code or resources.

   Explicit overrides must be authoritative: current `locate_optional_tool`
   discards a missing override and searches elsewhere. Adjust the FFmpeg/ffprobe
   resolution path to return an error for a configured missing tool. Do not
   introduce a fallback that hides a damaged production bundle. Keep unrelated
   optional-tool discovery behavior scoped to its existing callers.

3. In `crates/encap-video/src/lib.rs::renderer`, replace the explicit-path
   `ToolDiscovery` wrapper with:

   ```rust
   MediaTools::discover_with_validator(|host| {
       let tools = avid_core::MediaTools::from_paths(
           host.ffmpeg(), host.ffprobe(), cancellation,
       ).map_err(EncapError::from)?;
       Ok(Renderer::new(tools))
   })
   ```

   Here `MediaTools` is the existing `encap_ffmpeg::MediaTools` host adapter.
   Remove the unused `ToolDiscovery` import. Keep the same cancellation token
   through Core validation and media calls. Audio/Transcript should validate the
   host pair under EnCAP policy too; Core's Video validator alone is not all-mode
   qualification. There is no need for a second pair or duplicate media graphs.

4. Remove or rename the obsolete `managed-runtime` feature in
   `crates/encap-ffmpeg/Cargo.toml` and `crates/encap-engine/Cargo.toml` and update
   its callers/tests. The optional avid-core dependency in encap-ffmpeg can be
   removed if only Video calls Core, or made ordinary if EnCAP chooses to share
   pair validation across modes. Keep the existing unrelated process runner and
   cancellation behavior unless separately migrating it.

5. Remove `script/acquire_core_runtime.sh`'s call to Core acquisition. Retire or
   rewrite `check_core_runtime.sh`, `test_core_revision.sh`,
   `test_core_runtime.sh` and `package_core_candidate_macos.sh`. Replace Core
   manifest/candidate checks with host executable, integrity and packaged-mode
   checks, including missing/damaged/mismatched tool rejection. Remove the helper
   checkout gate from `script/build_and_run.sh` and
   `.github/workflows/build-platforms.yml`. Cargo's pinned source and lockfile
   provide source reproducibility. Optional Core test checkouts can remain pinned
   to the same revision, but must not provide acquisition/packaging helpers.

6. Preserve and review EnCAP-owned `fetch_ffmpeg.sh`, `prepare_ffmpeg.sh`,
   `build_ffmpeg.sh`, `build_ffmpeg_linux.sh` and Windows/Linux workflow inputs.
   They are a starting point for host packaging, not a Core-approved release.
   EnCAP pins its chosen sources/checksums and verifies the union of Audio,
   Video and Transcript requirements, including libx264/libx265/AAC and its
   audio codecs. Update license copying from `../AVID Core/LICENSE` to a
   host-maintained notice copy or resolved pinned dependency source. Keep source
   compliance, signing, notarization and production qualification in EnCAP.

7. Keep `encap-core`'s `VideoSettings` / `VideoProjectState` re-exports, schema
   validation, unknown-field preservation and EncapError mapping. Keep Video's
   chapter-source mapping, main/chapter artwork fallback, selection ordering,
   initialized-empty selection semantics, protected project/source paths,
   `Input::Timeline`, SquarePadded composition, stereo/48 kHz sequence audio,
   codec/encoding choices and hvc1 tag. Do not change `.encap` archives, native
   playback, stdout JSON or Audio/Transcript behavior as part of runtime ownership.

8. Run `cargo fmt --all -- --check`, `cargo check --workspace --locked`,
   `cargo test --workspace --locked`, `cargo test --workspace --locked --all-features`,
   and `cargo clippy --workspace --locked --all-targets --all-features -- -D warnings`.
   Run engine `media_contract` with its existing explicit `ENCAP_TEST_ENGINE`,
   `ENCAP_FFMPEG`, `ENCAP_FFPROBE` inputs after adapting managed-only assertions.
   Run `save_protocol`, `audio_edit_protocol`, project serialization/migration,
   Video adapter and Audio/Transcript tests. Run Core's five real-media tests
   against EnCAP's selected pair via `AVID_TEST_FFMPEG` and `AVID_TEST_FFPROBE`.
   Qualify actual packages across supported platforms: all modes, open/save,
   H.264/HEVC, chapter timing/order/artwork, cancellation, failure preservation,
   absent/corrupt/mismatched tools, signing/notarization, updater and UI.

Acceptance: a clean EnCAP checkout builds from the pinned Git dependency without
an adjacent Core checkout or Core runtime release. Every production mode uses
EnCAP's selected pair, with host-owned packaging and qualification. No EnCAP
acceptance task blocks Core commits or source tags.
