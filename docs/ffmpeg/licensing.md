# FFmpeg licensing audit

This is an engineering inventory, not a legal conclusion about application distribution or patent obligations.

## Evidence and configuration decision

[FFmpeg's legal page](https://ffmpeg.org/legal.html) describes the default LGPL licensing and GPL effect of optional components. The selected official source includes `LICENSE.md`, `COPYING.LGPLv2.1`, `COPYING.LGPLv3`, `COPYING.GPLv2` and `COPYING.GPLv3`; consult the source-specific `LICENSE.md`, which enumerates external-library compatibility and configure flags.

The current hosts already require GPL software video encoders. Core uses `--enable-gpl`, `--disable-version3`, `--disable-nonfree`. This source profile builds successfully with those flags. Dropping GPL/libx264/libx265 would remove existing software H.264/HEVC behavior. Nothing in this task authorizes nonfree components, and both validation and tests reject them.

| Component | Source license evidence | Why included / consequence |
| --- | --- | --- |
| FFmpeg | Official release COPYING files and LICENSE.md | Primarily LGPL-2.1-or-later; selected GPL components change FFmpeg executable licensing to GPL. Exact distribution obligations require source-specific review. |
| x264 | Official VideoLAN revision, COPYING and file headers | GPL-2.0-or-later; required by software H.264 command. No proprietary x264 license is assumed. |
| x265 | Official VideoLAN release, COPYING and file headers | GPL-2.0-or-later open-source terms; required by software HEVC command. No commercial license is assumed. |
| LAME | Official SourceForge release COPYING (GNU Library GPL v2), LICENSE | LGPL-2.0-or-later library; required MP3 encoder. Static redistribution must account for LGPL source/relinking obligations in the combined distribution. |
| zlib | Official archive README/zlib.h license | zlib license; required PNG codec compression. Preserve notice. |
| NASM | Official release LICENSE | BSD-2-Clause assembler, build-only; source retained for reproducibility. |
| CMake | Official Kitware build tool | BSD-3-Clause; build-only, not bundled as a media runtime. |
| Apple frameworks | Apple SDK/system | AudioToolbox/VideoToolbox are linked system interfaces. Review SDK and application-distribution terms; no third-party framework copy is bundled. |
| Compiler/system runtime | Per target SDK, libc/libc++/libstdc++/Windows CRT | Record imports and environment; static compiler runtime licensing exceptions and OS redistribution rules need target review. |

Do not confuse the Core crate's existing GPL-3.0-only license with the selected executable's license or use Core's LICENSE as a substitute for FFmpeg's actual licenses. ATIV's current macOS packaging defaults to such a substitute; the handoff requires replacing that input with the actual runtime notices.

## Flags and linking

- `--enable-gpl` enables components whose combined FFmpeg license is GPL; x264/x265 need this for the preserved software encoder choices.
- `--enable-version3` permits components that require upgrading the applicable license version. It is not needed by this candidate and remains disabled. Adding oneVPL or other SDK libraries may change this assessment; consult the pinned version's license and FFmpeg configure checks.
- `--enable-nonfree` permits incompatible combinations; FFmpeg's source documentation describes resulting binaries as unredistributable. It is prohibited without explicit user approval; a successful compile does not establish redistribution rights.
- Static versus shared FFmpeg changes packaging and compliance work. The Rust core invokes standalone executables and does not link FFmpeg libraries into Rust. Codec libraries are statically linked into the media executables, with only permitted system linkage. Process separation is an architectural fact, not a legal conclusion about combined works.

See also [LAME's official download page](https://lame.sourceforge.io/download.php), [GNU LGPL v2.1](https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html) and [GNU GPL v2](https://www.gnu.org/licenses/old-licenses/gpl-2.0.html). License texts from exact source packages are shipped alongside the executables. zlib's license is in `zlib.h`/README, so it must be copied explicitly as well as COPYING-style files.

## Before release: human review

Verify the exact combined-license designation, notice wording, source availability/relinking approach, whether an offer or accompanying source is appropriate, target compiler runtimes and application-store constraints. The proposed release includes corresponding source and build scripts on the same release as binaries; hosts must maintain that relationship and retention rather than assuming an expiring CI artifact satisfies obligations. Review any patches and new hardware SDK licenses before enabling them. H.264, HEVC, AAC and other standards may involve patent questions separate from copyright licensing; jurisdiction/use/distribution-specific conclusions require qualified review.

## Recipe 7 compiler runtime notices

Windows packages now copy the actual installed MSYS2 CLANG64/CLANGARM64 `crt`, `headers`, `winpthreads`, `compiler-rt`, `libc++` and `libunwind` notices into `licenses/toolchain/`, hash them in `build.json`, and include them in corresponding-source archives. Package versions are recorded in `system_packages`. MSYS2's [CRT recipe](https://github.com/msys2/MINGW-packages/blob/master/mingw-w64-crt/PKGBUILD) installs the MinGW runtime notices; its [libc++ recipe](https://github.com/msys2/MINGW-packages/blob/master/mingw-w64-libc%2B%2B/PKGBUILD) installs the LLVM runtime notices. Build-only tool executables are not distributed. Linux and macOS continue linking their recorded system runtime libraries without copying those libraries.

Each runtime binds an accompanying source archive containing exact FFmpeg/dependency sources, the recipe and applied patch logic, Core source/tests and notices. Retain and distribute this source mapping together with binary distributions. Application store, signing and application-level licensing decisions remain downstream responsibilities.
