# Recipe 6 repair audit — 2026-09-16

Audited before implementation changes at Core `eab97dd043187aa8b7a1cae4eb2c1228fa25a9db` (0.2.1). Scope is AVID Core only. This is an investigation record, not production qualification.

## Pipeline inventory

| Layer | Current implementation and finding |
| --- | --- |
| Source acquisition | `scripts/ffmpeg/build.py::download/source`; HTTPS official FFmpeg archive, checksum-verified cache; x264 immutable Git commit export. No prebuilt media runtime. |
| Version pin | `runtime/ffmpeg/spec.json`: FFmpeg 9.0.1, recipe 6, tag n9.0.1, revision bf1b838f2ab88b4f8fd83443325c782ea0e0f7fa. Archive VERSION and both executable versions checked. |
| Source verification | Archive SHA-256 cf38e0e28c7e5605942c4a77755349b0145804a397af37eb1fb4c77cb237f635; `provenance.py` checks signed release, signed tag, fingerprints, commit, epoch and archive/tag file contents and executable modes. |
| Configure policy | Static FFmpeg; explicit FFmpeg/FFprobe, no ffplay; autodetect/network disabled; file/pipe only; native decoders/demuxers retained; explicit software encoders and composition filters. Full invocation in build.json and validation.json. |
| Toolchain | Apple clang on macOS; GCC on Ubuntu; MSYS2 clang/LLVM on Windows. CMake 3.31.6 archive pinned by target/checksum; native tools recorded. Hosted images/MSYS2 packages are recorded, not immutable inputs. |
| Dependencies | x264 b35605ace3ddf7c1a5d67a2eb553f034aef41d55, x265 4.1, LAME 4.0, zlib 1.3.1; NASM 2.16.03 build-only. No Windows/Linux GPU SDK dependencies in recipe 6. |
| Linking | Static media/codec libraries, permitted system runtimes only. Windows uses -static and no PE timestamp; x265 pkg-config patch preserves LLVM unwind linker options. Validation checks imports with llvm-objdump, ldd or otool. |
| Architecture/CPU | Native architecture checks and binary PE/ELF/Mach-O machine checks. Windows compiler triple checked. x86 NASM and codec assembly enabled; no explicit -march=native in Core flags. CPU-feature dispatch has not been isolated as the crash cause. |
| Windows environment | windows-2025 x64 / windows-11-arm ARM64; MSYS2 CLANG64/CLANGARM64. MSYS Python builds; native Rust runs integration tests. Target .exe suffix and Windows path conversion are explicit. |
| Linux environment | Ubuntu 24.04 x64/ARM64; build-essential/pkg-config; static media libraries with glibc/compiler runtime linkage permitted. Both latest source-runtime jobs passed. |
| macOS environment | macos-15 ARM64 / macos-15-intel; deployment target 13.0; Apple system frameworks; optional VideoToolbox, required software encoders. Content-derived Mach-O UUID and ad-hoc signature normalization. |
| Staging | build.py copies the exact stripped FFmpeg and FFprobe pair to dist/<artifact>; writes spec/build/source provenance and actual upstream license texts. |
| Runtime artifacts | package.py validates metadata, binary hashes, test marker and repeat result; writes internal SHA256SUMS, normalized tar/gzip and detached checksum. |
| Source artifacts | Pristine archives/Git exports, recipe scripts, specification, signing certificates/signature, Core source/tests and licensing notice. SOURCE.json binds source archive SHA-256 to runtime. |
| Discovery | Managed Rust APIs require exact specification/source/recipe/target and versions/capabilities with explicit paths. Authentication belongs to acquire.py; generic discovery API retained for compatibility. |
| Qualification | validate.py checks versions, architecture, capabilities, linkage, audio/video/probe/artwork/metadata; five real-media Rust tests add composed pixels/cadence/audio comparisons. Baseline reports cannot qualify packages. |
| Lifecycle | tests/lifecycle.rs uses generated POSIX shell fixtures and is excluded on Windows. It tests renderer cancellation, fallback, output replacement and cleanup, not the complete installed-runtime update lifecycle. Windows native lifecycle coverage and acquisition/replacement verification remain gaps. |
| Provenance | build.json records Core commit, dirty state, spec/script hashes, configure/environment, tool versions and runner/package inventory. source-provenance.json records upstream verification. |
| Repeated builds | repeat.py rebuilds from fresh trees at the same absolute prefix, validates the second pair and compares full executable hashes. First qualified pair is packaged; repeat output is not promoted. |
| Publication | ffmpeg.yml builds all six targets, then promote validates complete runtime/source packages and hash-bound qualification evidence. Attestation precedes gh release create; no post-qualification rebuild in publication job. acquire.py requires immutable release and verifies attestation/asset digests. Repository immutability configuration still needs verification. |
| Release blockers | spec is candidate; minimum-OS/toolchain/host evidence incomplete. Checked-in evidence must bind exact published binary hashes. Moving qualification evidence into a new checkout currently interacts with strict Core revision/spec/script matching and needs an explicit promotion design. No runtime releases returned by repository release API during audit. |

## Observed failures

[Runtime run 35065840420](https://github.com/tlolabs/avid-core/actions/runs/35065840420), exact audited commit: macOS ARM64/x64, Linux ARM64/x64 and Windows ARM64 passed build jobs. Windows x64 failed the Rust media suite, not startup validation.

Windows job 104695838344: `cargo test --locked --test ffmpeg -- --ignored --test-threads=1`, test `simple_matches_legacy_pixels_audio_and_requested_cadence`, tests/ffmpeg.rs:328. FFmpeg returned 3221225477 (`0xC0000005`) during the second (portrait 90x160, 60 fps, both flips) legacy render. Stderr only reports guessed mono channel layout. Full invocation is retained in `.ffmpeg-work/repair-audit/windows-x86_64-35065840420.log`. Version checks, software smoke and four other media tests passed. This narrows reproduction to the composed render; it does not identify a defective filter, codec or SIMD routine.

Original executable hashes from validation.json:

- ffmpeg.exe: e559803304c98e70ca1b649ccbbc8c6351895d7706aadf69d9742258cd40546c
- ffprobe.exe: 6dd4e1adb72b73979d301b424958078f827fdc6a9e4dd5e207e18efa7a6cf6a5

Compiler clang 22.1.7, x86_64-w64-windows-gnu, runner image 20260907.229.1. Imports reported only Windows API CRT, KERNEL32, SHELL32 and bcrypt. Diagnostic artifact 10434491687, SHA-256 4359eee51c920113009467b0e742134a00d7ca51950c88eb56b26f9697e97573, contains four JSON files; no media executable, debug executable, dump or config.log. The workflow only uploads runtime packages after success. A clean recipe-6 reconstruction must be compared against the original hashes before claiming it is the same binary.

[Ordinary CI run 35065840426](https://github.com/tlolabs/avid-core/actions/runs/35065840426), Ubuntu job 104695802492: `cargo test --locked --all-targets` fails in fixture setup at tests/lifecycle.rs:42, before cancellation/publication behavior. The fake `/tmp/.tmpVyrNhh/ffmpeg -version` cannot spawn: errno 26, ExecutableFileBusy, Text file busy. No FFmpeg source runtime is involved. Ten other lifecycle tests passed. Source-runtime CI runs tests serially; ordinary CI runs them concurrently. This is a fixture/process-spawn concurrency investigation, not evidence of a broken Linux FFmpeg binary. Root-cause validation still requires reproduction/tracing; retries or serializing qualification alone would not establish a fix.

Raw logs and downloaded diagnostic metadata are retained under `.ffmpeg-work/repair-audit/` in this Core checkout. No runtime flags, tests, API or release gates were changed during this audit.

## Ubuntu root cause and verified repair

The original parallel lifecycle suite reproduced errno 26 on its first execution in [diagnostic run 35120383566](https://github.com/tlolabs/avid-core/actions/runs/35120383566), this time during setup of `concurrent_operations_have_independent_cancellation_and_staging`. All 20 strace-instrumented repetitions passed; tracing altered scheduling and did not capture the transient failure. Do not describe those traces as proof of a specific descriptor inheritance event.

The race is between writing executable fixture inodes and spawning other processes concurrently. A fork can inherit a writable descriptor; closing the descriptor in the parent does not close the child's copy before exec. Linux then rejects execution of that inode with ETXTBSY. The mechanism and a reproducer are also documented in [Rust issue 114554](https://github.com/rust-lang/rust/issues/114554).

Repair commit `9ec92a4bba5b0951959fc51879b26d034727a5f7` replaces generated executable contents with symlinks to immutable checked-in executable fixtures. Test mode remains per-test ordinary data. No production invocation behavior, assertion, timeout, concurrency setting or media recipe changed. The slow-probe case replaces its symlink with the immutable sleep fixture.

[Linux regression run 35120734549](https://github.com/tlolabs/avid-core/actions/runs/35120734549) demonstrated the descriptor mechanism under a controlled fork: writable executable -> errno 26; immutable executable with an inherited writable data descriptor -> exit 0. All 100 independent executions of the parallel 11-test lifecycle suite passed (1,100 test cases). Artifact `linux-lifecycle-regression`, ID 10457385274, SHA-256 b4aff07068a75f9aa8633e8b75183632a0a16f70358f778838f81d436798e5da. Local formatting, 42 default tests, Clippy, five real-media tests and two managed-runtime tests also passed against the existing macOS ARM64 recipe-6 pair.

## User-authorized OS qualification scope

The user has no immediately available macOS 13 or Windows 10 1809 test machines and explicitly authorized using the hosted runners' OS versions for this release qualification. Final release records must identify the observed hosted OS/image for each target; they must not claim execution on the former minimum OS versions. This scope authorization does not waive any binary, media, lifecycle or provenance failure.

## Windows root cause: compiler-generated unaligned SSE2 conversion

[Exact reproduction run 35120383566](https://github.com/tlolabs/avid-core/actions/runs/35120383566) reconstructed recipe 6 at its original prefix and **both executable SHA-256 values exactly match the failed job above**. Artifact `unqualified-windows-recipe6-reproduction` (10457266647, SHA-256 13753452a1956e77fbc4f816c5d4dbe4ca98b5f5e7ec9338d6fa4c8c9ab4b9c9) retains executables, debugger logs, disassembly, commands/stdout/stderr/statuses and build metadata.

Small reproducer:

```sh
ffmpeg.exe -v error -f lavfi -i testsrc2=s=90x160:d=0.1 -vf gblur=sigma=40 -frames:v 1 -f null -
```

This fails with 0xC0000005; width 96 passes. Versions, codec/format lists, generated audio/video encoding, decoding and probing pass. Scale/crop without blur passes. Disabling FFmpeg runtime CPU dispatch (`-cpuflags 0`) and setting one filter thread still fail. Thus no external encoder, Core invocation, input file, DLL loading failure or FFmpeg-selected assembly routine is needed to reproduce the crash.

LLDB stops in `filter_frame` at the C output conversion loop in `libavfilter/vf_gblur.c` (`dst[x] = lrintf(bptr[x])`). Faulting instruction is `cvtps2dq (%r11,%rdi,4), %xmm0` at preferred address 0x14009f630 (`filter_frame+0x520`). Captured r11=0x142fcc19fa8 and rdi=0 select an address aligned to eight bytes, not sixteen; a 90-float row advances 360 bytes. The legacy SSE2 memory instruction requires sixteen-byte alignment. The reported read address of all ones reflects the resulting protection fault, not a literal invalid pointer in the filter.

The clang 22.1.7 X86 backend's `X86InstrSSE.td` SSE2 pattern folds `lrint(loadv4f32)` into `CVTPS2DQrm` without requiring aligned memory. The same pattern occurs in inspected LLVM 21.1.8 and 22.1.8 sources; a version change alone is not an established fix. [Pinned LLVM source](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.7/llvm/lib/Target/X86/X86InstrSSE.td).

The standalone C reproducer in `tests/fixtures/compiler/lrintf-alignment.c` isolates this without any FFmpeg, codec library or Core code. [Compiler regression run 35122354184](https://github.com/tlolabs/avid-core/actions/runs/35122354184), artifact 10457981837 (SHA-256 15be0fdf2234ba15faa3727af34e35078f90f926172a8702c8e0b0364a9cbd29): stock `-O2 -fno-math-errno -static` passes widths 88/92/96 and crashes at 89/90/91/93/94/95. Adding only `-fno-builtin-lrintf` passes all widths with correct converted pixels. Disabling loop/SLP vectorization also passes but is broader and is not selected.

Recipe 7 adds only `-fno-builtin-lrintf` to Windows x64 compiler flags, using the CRT scalar implementation for this function while retaining other vectorization, NASM, codecs, formats and CPU dispatch. The build executes the standalone regression, and every target's native FFmpeg smoke exercises blur widths 88–96. Full clean native media, lifecycle, archive-installation and repeated-build qualification remains required before release.

Controlled FFmpeg object experiment [run 35122083642](https://github.com/tlolabs/avid-core/actions/runs/35122083642) completed: recompiling stock `vf_gblur.o` reproduces the exact original executable SHA-256 and width-90 crash. Recompiling only that object with `-fno-builtin-lrintf` passes widths 90 and 96. `-O1` and disabling vectorization also pass; disabling the peephole optimizer still crashes. Every variant uses the same static libraries, assembly, architecture/configuration and linker environment. Artifact 10458153163, SHA-256 f02a0e89b54f5e316a26ccf52be860637ddc6a94754a6048649bff19cb5ca3e3. This establishes the compiler transformation as the failure origin and the function-specific flag as the narrow demonstrated repair.

## Recipe 7 native results and Windows checkout defect

[Qualification run 35124391473](https://github.com/tlolabs/avid-core/actions/runs/35124391473), Core `9acf49d826fc27b057e46aa2c65ad5ce14693fff`: macOS ARM64/x64 and Linux ARM64/x64 passed the complete native pipeline, including archive installation and byte-identical clean repeated executables. Both Windows targets passed all five real-media tests, all three managed-runtime lifecycle tests and byte-identical repeated executable builds. In particular, Windows x64 `simple_matches_legacy_pixels_audio_and_requested_cadence` passed unchanged at 17:05:52 UTC.

Both Windows jobs were correctly rejected during final archive qualification by `python3 scripts/ffmpeg/qualify_archive.py dist/packages/avid-ffmpeg-9.0.1-r7-<target>.tar.gz`: `ValueError: Release build was not clean`. Their `build.json` records `core_worktree_modified: true`; the original recipe-6 Windows metadata also recorded true. No Windows archive from this run is eligible for publication. Failed-job executable/metadata artifacts are 10459667984 (x64, SHA-256 d65177ff368f7a06173845ca0a63dfdf7765d1d0696ade7ef3f8c46ccb2af281) and 10460265629 (ARM64, SHA-256 761f8091c4145a6f0ad5df9c9c7ce03ef009f8eb371040dace0d7fa19e3e407e).

[Focused checkout diagnostic 35127745479](https://github.com/tlolabs/avid-core/actions/runs/35127745479) shows Git for Windows considers its checkout clean, while MSYS Git reports Rust, Cargo, Markdown and other text files modified before any build or test. File modes are unchanged. The existing `.gitattributes` forced LF only for Python, JSON, shell, YAML and key files; the remaining source files were subject to Git for Windows line-ending conversion and different MSYS defaults. The recipe/script files already matched canonical Git bytes, explaining why that narrower identity check did not catch it.

Repair: apply `text=auto eol=lf` repository-wide so all text source bytes are identical across the two Git implementations and Unix. Binary detection remains automatic. CI now rejects a dirty checkout before compilation and records the exact Git status instead of only a boolean. The dirty-source release guard remains mandatory. The fast Windows diagnostic asserts a clean checkout and verifies that an intentional source edit is detected before restoring the original bytes. A new complete qualification run is still required; prior failed Windows artifacts cannot be relabelled or promoted.

Recipe-7 software, hosted-OS and toolchain evidence is verified directly from native build/validation/repeat/archive receipts; historical recipe-6 ledger entries cannot substitute for those results. The existing application packaging prerequisite remains required by `qualification_policy.host_packaging` pending the user's separate release-scope decision.

## Final observed blocker

The canonical LF checkout regression passed on Windows in run 35128192733. Subsequent complete qualification run 35128507238 passed both macOS, both Linux and Windows ARM64 targets, but Windows x64 failed runtime-directory replacement with OS error 5 after all media tests passed. Focused exact-binary reproduction 35130838334 failed on iteration 71; a scoped Restart Manager snapshot identified a live `provjobd.exe3702017752` resource owner. This is evidence of possible runner interference, not proof of the precise failing handle. No retry or suppression was added. See the [complete blocker report](qualification-blocker-2026-09-16.md) for commands, logs, binary/artifact hashes, scope limitations and the release boundary. No runtime release was published.
