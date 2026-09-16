# Recipe 7 native qualification and release status

All six clean native targets passed [qualification run 35157560636](https://github.com/tlolabs/avid-core/actions/runs/35157560636) at **`fab2ed86bb64582d3f7a7dd736c713cc3951550b`** (Core 0.2.2 source, FFmpeg 9.0.1, Recipe 7). Later documentation commits do not change the identity of these qualified artifacts.

Each target passed all five media tests, all four installed-runtime contract tests, required software/capability/linkage checks, and comparison of two clean builds. The first executable pair was packaged and that exact archive was subsequently installed and tested. Independent local checks verified the retained Actions artifact digest, archive checksums, corresponding source, native host, script/source identity, executable hashes and installation receipt. [Machine-readable target evidence](evidence/native-qualification-r7.json) records the exact hashes and job identities.

The Windows x64 pair is byte-identical to the pair used by the [1,500-cycle corrected lifecycle stress run](evidence/windows-lifecycle-stress-r7.json). That run verified 53,250 fully released child processes and recovered 41 transient Windows directory operations without a failed cycle. [The investigation](windows-lifecycle-investigation.md) documents the external `provjobd` read handles, bounded Windows behavior, resource audit and remaining historical cleanup question.

## Executed platform scope

| Target | Native qualification host | Result |
| --- | --- | --- |
| macos-arm64 | macOS 15.7.9 | Passed |
| macos-x86_64 | macOS 15.7.9 | Passed |
| windows-x86_64 | Windows 2025Server 10.0.26100 | Passed |
| windows-arm64 | Windows 11 10.0.26200 | Passed |
| linux-x86_64 | Ubuntu 24.04.5 LTS | Passed |
| linux-arm64 | Ubuntu 24.04.5 LTS | Passed |

These are the hosted OS versions authorized for this task. They do not establish execution on macOS 13 or Windows 10 1809. Optional hardware encoding is outside required software qualification.

## Retained artifact checksums

Names use `avid-ffmpeg-9.0.1-r7-<target>.tar.gz` for runtime and `avid-ffmpeg-9.0.1-r7-<target>-sources.tar.gz` for source. These are qualified Actions candidates, not published production downloads.

| Target | Runtime SHA-256 | Source SHA-256 |
| --- | --- | --- |
| macos-arm64 | `b029c0f44f888e2d686bca0e22e0687164f80da28e5822fe5be3b7e3b3f529df` | `77d0732976e25903c2f10b460c3a7b7db5a938045cb7a5b3acdbe8cd6ceef03b` |
| macos-x86_64 | `45aefa885d02c65dd6b5cc0a800d28734f3736385356e8e4d69e4c38081df5b4` | `77d0732976e25903c2f10b460c3a7b7db5a938045cb7a5b3acdbe8cd6ceef03b` |
| windows-x86_64 | `1924663e2c3649614a275bf48c4a69d4aaaddd1346a038cda7977a311f49d63c` | `a676c3b40288048c32014839495b3f05ec005a162dd7bbe164d71e1888a171a5` |
| windows-arm64 | `2af04f658cba2dd6742651fc62207a15fd0170f6109740cc96096cc80c5acae3` | `a676c3b40288048c32014839495b3f05ec005a162dd7bbe164d71e1888a171a5` |
| linux-x86_64 | `7e7f55a445a6c5815d9753cfa74ebb61e68c96fae1aa8b271bbc3749c48ed4fd` | `77d0732976e25903c2f10b460c3a7b7db5a938045cb7a5b3acdbe8cd6ceef03b` |
| linux-arm64 | `da2bc9b680827038be53f6b9d7ad8348fbfdf0f6c162a377b4091d2c508b25f2` | `77d0732976e25903c2f10b460c3a7b7db5a938045cb7a5b3acdbe8cd6ceef03b` |

## Publication and downstream handoff

**No runtime release or Core v0.2.2 tag has been published.** The exact-artifact publication preflight currently refuses promotion with `Unperformed qualification gate: macos-arm64 host_packaging`; the existing `qualification_policy.host_packaging` is `required`. macOS ARM64 application evidence is incomplete, and the other application packaging gates are not run. Core runtime success does not substitute for app signing, packaging, launch or update qualification.

The release policy decision remains pending: retain application packaging as a prerequisite, or explicitly authorize a Core-only runtime release while preserving those application checks as downstream requirements. Separately, the isolated historical late-cleanup failure remains unattributed after 3,000 additional historical full cycles and 1,000 reduced cycles failed to reproduce it. Its original source line and numeric error were not retained by the initial privacy allowlist. [The precise evidence gap and reproductions](evidence/windows-historical-cleanup-r7.json) remain explicit; this failure is not hidden by the corrected stress result.

The intended immutable runtime identifier is `ffmpeg-9.0.1-r7`, paired with Core `v0.2.2` at the exact qualified commit above. These identifiers are reserved by the specification, not a claim that they currently exist as releases. Once authorized and all blockers are closed, promote this existing run through `promote_run_id=35157560636`; do not rebuild FFmpeg for publication. The final release must contain `manifest.json`, `SHA256SUMS`, the six runtime/source pairs, qualification receipts and GitHub provenance attestations.

After publication, downstream sessions must check out the matching Core tag and use `python3 scripts/ffmpeg/acquire.py <target> <new-destination>` there. Acquisition authenticates the immutable release, attestation and exact runtime/source/manifest mapping, and rejects existing destinations or unqualified releases. Follow [the migration contract](migration.md) for staging, notices, host signing, stopping/joining workers and using the additive bounded directory APIs. Existing colocated application layouts also require the application and engine processes to exit before whole-application replacement. Do not use these Actions candidates for production or add a PATH fallback. ATIV and EnCAP were not modified.
