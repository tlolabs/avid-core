# Core FFmpeg runtime architecture

## Before this change

Core 0.3.0 accepts caller-selected FFmpeg and FFprobe paths. `MediaTools::from_paths`
checks their identities and matching version strings, but does not authenticate a
Core build. Optional development discovery can search adjacent directories and
PATH. Core's source release contains no runtime.

The retained Recipe 7 pins FFmpeg 9.0.1 (`n9.0.1`, revision
`bf1b838f2ab88b4f8fd83443325c782ea0e0f7fa`) and source dependencies in
`runtime/ffmpeg/spec.json`. `build.py` verifies upstream release and tag
signatures, compiles both CLI programs in one configure/build, and copies both
into a target-specific candidate directory with build metadata, source mapping,
licenses, and corresponding source. `validate.py` checks native architecture,
versions, capabilities, linkage, hashes, and real media operations; FFprobe
already inspects audio, artwork, chapters, and encoded video. `repeat.py`
compares two clean builds of both binaries. `artifact.py` retains strict archive
and evidence checks, but the package/promotion tools were made historical.

The manual `ffmpeg.yml` workflow runs Recipe 7 across six native runners and
keeps diagnostics. `ci.yml` tests the Rust library with system tools on one
external-media job. Lifecycle tests exercise both process names and Windows
directory replacement/cleanup. These jobs do not currently publish or qualify
a Core runtime artifact. Qualification records still mark most target and host
packaging gates incomplete or not run. The Rust crate is GPL-3.0-or-later;
Recipe 7 enables GPL codecs and explicitly disables nonfree components.

## Change plan

Keep the source pin, single build, native matrix, media tests, lifecycle fixes,
and existing provenance format. Add a fail-closed Core directory constructor
that checks the packaged pair and evidence, while preserving the path-based API
for compatibility. Restore candidate archive packaging through the retained
archive validator and make manual CI upload that archive only after tests and
repeat-build checks pass. Do not treat a candidate as a release or assert target
qualification before native gates pass.
