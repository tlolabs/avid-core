# Software source runtime: local qualification results

Recipe 6 removes Windows/Linux GPU SDKs, required GPU capabilities and hardware-parity promotion gates. Software encoding is the reference path on all six targets. Optional macOS VideoToolbox is recorded separately. macOS 13, Windows 10 1809 and the Ubuntu 24.04 glibc/toolkit baseline remain required.

## Verified locally

- Two clean macOS ARM64 source builds produced byte-identical FFmpeg and FFprobe. Upstream release/tag signatures and source-tree equivalence passed. Content-derived Mach-O UUIDs and deterministic ad-hoc signing eliminate linker identity variability; the repeat check compares complete executable bytes.
- 17 infrastructure tests, 42 Core default/process tests, five real-media tests and two managed-runtime integration tests passed.
- Software H.264/HEVC, audio codecs, artwork, metadata/chapters, progress and transcript preparation passed.
- Optional VideoToolbox H.264/HEVC encode, stream inspection and full decode passed. No software/hardware byte or file-size comparison is required.
- ATIV signed candidate: 27 presets, 20 previews, three exports, failure/cancellation cases and two native process tests passed.
- EnCAP signed candidate: three all-mode media/persistence/recovery tests, three save/edit protocol tests and 26 native tests passed.
- Both hosts rejected missing/damaged managed runtimes with a usable external pair on PATH. Both signed candidate apps launched and remained running.
- Original hashes were verified before host signing. Signed hashes were recorded separately. Runtime data resides in macOS Resources; executable helpers remain in Contents/MacOS.

## Evidence boundaries

These are local ad-hoc-signed qualification apps, not production releases. Manual UI/accessibility, authenticated upgrades, production signing and exact minimum-OS runtime qualification remain incomplete. The tools advertise macOS 13 as their deployment target; execution on macOS 26.7 does not qualify macOS 13.

The source runtime was built at `9eb8651bc09b6fb007784d3df840d51fa7bbba82`. The additive metadata-layout API was then tested at `9554818` against the same pair. That API addition did not change the FFmpeg specification or build scripts. Host source changes were local during candidate tests; final workflow pins follow the reviewed Core commit.

The qualification branches were pushed with explicit user authorization on 2026-09-15 (2026-09-16 UTC). Build-only CI was dispatched for [Core](https://github.com/tlolabs/avid-core/actions/runs/35064874866), [ATIV](https://github.com/tlolabs/ativ/actions/runs/35064891745) and [EnCAP](https://github.com/tlolabs/encap/actions/runs/35064906664). Core publishing and ATIV production signing are disabled; no release was published. These run links record dispatched work, not passed qualification. Windows make targets include the required `.exe` suffix; its native CI result is still pending. Normal release acquisition remains in place until independent qualification gates are satisfied; each host has candidate package/test scripts.

See [the ledger](../../runtime/ffmpeg/qualification.json), [software evidence](evidence/macos-arm64-software-r6.json), [optional VideoToolbox evidence](evidence/macos-arm64-videotoolbox-r6.json) and [candidate host observations](evidence/macos-arm64-host-observations-r6.json).
