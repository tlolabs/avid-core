> Historical FFmpeg build/qualification research, superseded by Core 0.3.0.
> These requirements do not gate Core source releases or host builds.
> See [the current integration contract](../integration.md).

# Recipe 7 qualification blocker — 2026-09-16

> Historical blocker snapshot. The subsequent Windows lock diagnosis, corrective API and 1,500-cycle stress evidence are recorded in [the lifecycle investigation](windows-lifecycle-investigation.md). This document does not describe the current release-candidate status.

**No production runtime release is published. ATIV and EnCAP must keep their migration gate closed.** Core 0.2.2 is prepared on `codex/ffmpeg-runtime-qualification`; there is no new Core version tag or runtime release identifier to consume.

The original Windows x64 access violation and Ubuntu lifecycle fixture race are diagnosed and repaired. The [repair audit](repair-audit-2026-09-16.md) records exact original binaries, debugger evidence, controlled compiler experiments and the Linux descriptor reproducer. Recipe 7 retains codecs, formats, assembly and existing managed-runtime APIs. Windows source checkout line-ending normalization is also repaired and verified.

## Current native qualification

[Run 35128507238](https://github.com/tlolabs/avid-core/actions/runs/35128507238) built source revision `16ae29e80b6adc3c003ce18f363b3ce9f3257884`. These results apply to that revision, not later diagnostic/documentation commits.

| Target | Observed hosted OS | Result |
| --- | --- | --- |
| macos-arm64 | macOS 15.7.9 | Complete native pipeline passed |
| macos-x86_64 | macOS 15.7.9 | Complete native pipeline passed |
| windows-arm64 | Windows 11, 10.0.26200 | Complete native pipeline passed |
| windows-x86_64 | Windows Server 2025, 10.0.26100 | Media passed; runtime directory replacement failed |
| linux-x86_64 | Ubuntu 24.04.5 | Complete native pipeline passed |
| linux-arm64 | Ubuntu 24.04.5 | Complete native pipeline passed |

The five passing targets include clean repeated executable hashes and installation/testing of the final archived pair. Runtime/source archives and their qualification receipts were downloaded and independently verified against GitHub Actions artifact digests. [Machine-readable evidence](evidence/qualification-blocker-r7.json) records exact archive, source and executable hashes, artifact IDs and native OS evidence. These are retained candidates from an incomplete run, not release assets. macOS 13 and Windows 10 1809 execution remains untested under the user's authorization to use hosted OS versions.

## Exact outstanding failure

[Windows x64 job 104903441576](https://github.com/tlolabs/avid-core/actions/runs/35128507238/job/104903441576) fails:

```sh
cargo test --locked --test runtime_contract -- --ignored --test-threads=1
```

Test `installed_runtime_render_replacement_rollback_and_cleanup`, `tests/runtime_contract.rs:180` at the qualified-source revision: `std::fs::rename(&installed, &backup)` returns OS error 5, `PermissionDenied`, “Access is denied.” This occurs after rendering, probing, cancellation, timeout and discovery of a staged update. The original crashing media test passes. This is a distinct runtime replacement/filesystem blocker.

Exact failing-pair diagnostic artifact: [10460866442](https://github.com/tlolabs/avid-core/actions/runs/35128507238/artifacts/10460866442), SHA-256 `faba73e21ed3e9b467127c8229cb3d5af30b57cde3a4591edfe9244c03e8fae9`.

- FFmpeg: `35d37f442a44ceacd44a513b280835337eb01d45c593f565f283c3fd3781001d`
- FFprobe: `635148687f774de7e047e6b6ffcc41f627d644da3eff51f2ca52f4e661d1ac36`

[Focused reproduction 35130838334](https://github.com/tlolabs/avid-core/actions/runs/35130838334), diagnostic revision `5dc3df0e9ba655ee456f321c88673a7277952977`, verified and reused those exact binaries. It stopped at the first failure, iteration 71, using:

```sh
cargo test --locked --test runtime_contract installed_runtime_render_replacement_rollback_and_cleanup -- --ignored --exact --nocapture --test-threads=1
```

Log `test-71.log` in [artifact 10461846478](https://github.com/tlolabs/avid-core/actions/runs/35130838334/artifacts/10461846478), artifact SHA-256 `6e5a0a1a8aeda55c472627c7bc4ebdf95cbce3a7f03ea36fc37b3212fa116d63`, records:

```text
runtime replacement failed: installed ü runtime -> previous runtime: Access is denied. (os error 5)
test_executable=1 attributes=32 delete_handle_error=0
test_executable=2 attributes=32 delete_handle_error=0
RmRegisterResources=0
RmGetList populated=0 count=1
resource_owner_pid=7476 application=provjobd.exe3702017752 type=5 status=1 wait=258
```

The scoped Windows Restart Manager query registered only the two synthetic test executables. It reported a live external resource owner (`wait=258`, WAIT_TIMEOUT). This points toward runner-side resource interference, but it is a snapshot taken after the rename failed. Both executable DELETE-handle probes succeeded at that later instant. It does not establish the exact handle/share mode responsible for the directory denial, prove an antivirus cause, or conclusively exclude a Core handle problem. No owner was terminated; no broad process inventory, command lines or machine-wide file trace was uploaded. The test still fails unconditionally on the original rename error.

## Release and continuation boundary

Publication is blocked until the Windows x64 replacement failure is resolved and all six targets qualify together from one final clean Core revision. Passing another repetition alone is insufficient evidence of a repair. Further work must isolate the relevant directory/file handle or runner interference and validate the resulting repair without suppressing lifecycle errors. The available diagnostic identifies a suspected external subsystem but does not provide a justified source-code fix, so this attempt stops at this blocker as requested.

Release infrastructure now validates complete target evidence, exact archived payloads, corresponding sources/licenses, checksums, native host identity and archive-bound lifecycle receipts. It can promote a successful same-revision run without rebuilding and publish an attested immutable draft-to-final release. Repository immutable releases are enabled. Publication itself has not been exercised successfully because the required qualification run failed; no production attestation or release manifest exists yet.

The existing application-packaging prerequisite also remains `required`. The separate question about making ATIV/EnCAP application packaging a downstream gate has not been answered; promotion defaults preserve the prerequisite. Hosted-OS authorization does not waive it.

All implementation and diagnostic changes are committed on the repair branch. Both shared-Core CI runs at `5dc3df0` passed formatting, checks, default tests, Clippy and MSRV checks. The infrastructure suite has 19 passing Python tests. See [migration contract](migration.md) for the prepared acquisition mechanism; a production downstream handoff containing an actual release/tag/manifest must wait for publication. ATIV and EnCAP were not modified.
