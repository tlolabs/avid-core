# Recipe 7 migration contract

The release manifest is the production gate. Recipe/specification status `release-gated` alone does not permit migration. See [the runtime pipeline](README.md) and [repair audit](repair-audit-2026-09-16.md). A final downstream handoff will identify the published release and exact checksums after all required gates pass.

Core 0.2.2 preserves the Core 0.2.1 managed-runtime Rust API signatures. Hosts pin the matching Core source tag/commit, then run `python3 scripts/ffmpeg/acquire.py <target> <new-destination>` in that checkout. Acquisition needs Python 3.11+ and an authenticated GitHub CLI with attestation support. It verifies the qualified immutable release, attestation, checksums, native target, corresponding source and provenance. No PATH fallback is added and an existing destination is never replaced.

Keep `spec.json` and `build.json` with the binaries, or continue using `MediaTools::from_managed_layout` for split executable/resource directories. `host.py stage` verifies the acquired payload before application signing. Retain `release-manifest.json`, `acquisition.json`, original runtime notices and corresponding-source availability. Do not substitute the Core crate's license for the runtime's actual notices. Host signing changes executable hashes; retain original validation and use `host.py record-signed` for the signed hashes.

Qualified OS scope uses the hosted runner systems authorized by the user, not a claim of macOS 13 or Windows 10 1809 execution. Applications still own signing, notarization, installers, launch, UI and update tests. No ATIV or EnCAP files are changed by this Core task.
