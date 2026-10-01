> Historical FFmpeg build/qualification research, superseded by Core 0.3.0.
> These requirements do not gate Core source releases or host builds.
> See [the current integration contract](../integration.md).

# Current FFmpeg architecture and requirements audit

> Scope update (2026-09-15, recipe 6): software encoding is authoritative. Windows/Linux GPU interfaces and parity are not required. Historical hardware requirements below are superseded by [the current policy](README.md#configuration-and-dependencies). Minimum-OS qualification remains required. Results below describe earlier recipes unless explicitly identified otherwise.

Audit date: 2026-09-14. Scope: current working trees of AVID Core, ATIV and EnCAP, including uncommitted host work. The hosts are read-only for this change. `baseline/repository-audit.json` records HEADs, dirty-file lists and repository-wide FFmpeg/FFprobe/packaging references with source locations; `binary-locations.json` records executable paths, formats, sizes and SHA-256. Earlier Core extraction documentation is historical and predates this ownership refactor.

## Acquisition, discovery and packaging

| Area | Current ATIV | Current EnCAP |
| --- | --- | --- |
| Acquisition | `script/fetch_ffmpeg.sh` downloads binaries with pinned SHA-256 | Identical fetch script; Windows workflow additionally duplicates download URL/asset/digest directly |
| macOS provider | `ffmpeg.martin-riedl.de/download/macos/{amd64,arm64}/..._9.0.1`, separate ffmpeg.zip and ffprobe.zip | Same; exact URLs and digests in saved source-reference audit and fetch scripts |
| Linux/Windows provider | `github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-09-11-13-20/ffmpeg-n9.0.1-29-gad500d59cb-...` | Same; Windows workflow directly uses the release |
| Stable identity limitation | macOS identifies `9.0.1-https://www.martin-riedl.de`; other targets are **29 commits after n9.0.1**, not the exact stable tag | Same |
| Build entrypoints | Native release workflow fetches before testing/packaging | `build_ffmpeg.sh` and `build_ffmpeg_linux.sh` are now compatibility wrappers for `prepare_ffmpeg.sh`; they no longer compile FFmpeg |
| Local cache | `build/ffmpeg-<platform>-<arch>` | `.build-tools/ffmpeg-9.0.1-install-<arch>/bin`; fetch-recipe SHA and binary checksums validate reuse; stale cache replaced by prepare script |
| Runtime resolution | CLI `--ffmpeg` / `--ffprobe` into Core `ToolDiscovery`; explicit overrides fail if invalid | `ENCAP_FFMPEG` / `ENCAP_FFPROBE`, adjacent tools, sibling Resources, then PATH; invalid/missing env override currently falls through |
| Shared discovery | Explicit paths, supplied directories, engine-adjacent, adjacent `ffmpeg/`, Resources, Frameworks, then optional PATH | Video adapter resolves host pair and passes the same paths to Core; other modes retain encap-ffmpeg wrapper |
| Validation | Core checks executable identity and matching version identifiers; packaging additionally performs a real encode/probe | encap-ffmpeg generic validation only requires process success/nonempty output; Video uses stronger Core validation |
| macOS placement | `ATIV.app/Contents/MacOS/{ativ-engine,ffmpeg,ffprobe}` | `EnCap.app/Contents/MacOS/{encap-engine,ffmpeg,ffprobe}` |
| Windows placement | Adjacent to application/engine in publish directory and installer | Adjacent to application/engine in staging directory/archive |
| Linux placement | `usr/lib/ativ/{ativ-engine,ffmpeg,ffprobe}`; launchers and package/AppImage rules | `EnCap/bin/{encap-engine,ffmpeg,ffprobe}` within release tar |
| Signing | macOS codesign nested executables/app, notarytool/stapling, signed DMG/Sparkle; Windows signtool for executables/installer | Current macOS build copies tools, ad-hoc signs nested components/app and verifies; source audit found no equivalent complete Developer ID notarization pipeline for this FFmpeg change |
| Notices | Builds record `-buildconf`; optional downloaded license; macOS license input defaults to host LICENSE | Builds retain `-buildconf` and host third-party notices; migration must provide actual runtime license/source bundle |

The audit searches found no Cargo dependency linking libavcodec/libavformat into either host or Core. FFmpeg-family processes remain separate executables. Linux apt packages, macOS development tools, .NET, Swift and other package-manager dependencies support the applications/build environment; they are not the new canonical FFmpeg artifact. EnCAP also uses separate Whisper/Apple transcription tools and native audio playback; those are not FFmpeg dependencies to move into this recipe.

### Release targets and history

ATIV `.github/workflows/native-release.yml` distributes macOS ARM64/Intel, Windows x64/ARM64 and Linux x86_64/aarch64. EnCAP `.github/workflows/build-platforms.yml` distributes macOS ARM64/Intel, Windows x64/ARM64 and Linux x64. Native baselines include macOS 13 and Windows host minimum version declarations; retain them when qualifying toolchains.

EnCAP commit `46ffd34` unified distribution inputs with ATIV. Earlier commits `1031719`, `6f5159f` and `9f480cd` cover link-signing and the previous custom source build. Some older installed build trees persist; do not infer current packaging from those trees or the previous Core report. ATIV commit `3bc4e58` introduced its native release/download infrastructure. No host history was rewritten.

## Actual generated commands and compatibility requirements

| Source owner/location | Operation / flags | Required capability and parser contract |
| --- | --- | --- |
| Core `src/renderer.rs::probe_audio_duration` | ffprobe `-select_streams a:0 -show_entries stream=duration:format=duration -of default=noprint_wrappers=1:nokey=1` | Stream/format duration lines, finite positive parse; unknown duration remains optional |
| Core `src/renderer.rs::inspect_image` | ffprobe `-select_streams v:0 -show_entries stream=width,height -of csv=p=0:s=x` | Width/height integer CSV; bounded dimensions/pixels |
| Core `src/renderer.rs::capabilities`, `src/command.rs` | ffmpeg `-hide_banner -encoders` | Exact video encoder names/flags, not matches in descriptions |
| Core `src/command.rs::preview` | Artwork graph, `-map [video] -frames:v 1 -f image2` | Native PNG encoder, image2 muxer, input image decoder |
| Core `src/command.rs::export_with_mode`, single input | `-loop 1 -framerate`, two inputs, video graph, `-map [video] -map 1:a:0`, H.264/HEVC + AAC, yuv420p, `-shortest -movflags +faststart -f mp4` | libx264 with stillimage tune, libx265 with hvc1 tag, AAC/MP4, input sample rate/layout preservation |
| Core timeline export | Image/audio pairs with `-t`; duration trims, normalized 48k stereo, hard-cut concat, `-r` | Video trim/setpts, audio atrim/aformat/asetpts, implicit aresample, concat audio+video |
| Core graph modes | split, scale aspect ratio, crop, gblur, overlay, format, optional square pad and hflip/vflip | Shared composition filter graph; simple mode adds trim first frame, loop and setpts cadence |
| Core `src/progress.rs`, process wrapper | `-progress pipe:1 -nostats`, no stdin, direct OS-string arguments | `out_time_us`, legacy `out_time_ms`, `out_time`, speed, progress=end; bounded streams, cancellation, timeouts, child reap |
| EnCAP `crates/encap-audio/src/lib.rs::export_arguments` | Source audio inputs, artwork input, `-f ffmetadata` input, normalize/concat, `-map_metadata`, `-map_chapters`, channel count | WAV/AIFF, ffmetadata demux, aformat/asetpts/concat/resampling; MP3 or iPod/MOV M4A muxing |
| EnCAP Audio encoding | `libmp3lame -b:a ... -id3v2_version 3`; `aac` or `aac_at -b:a ... -movflags +faststart` | LAME, native AAC, macOS AudioToolbox AAC. EnCAP maps AAC export to M4A container; no encoding-default change |
| EnCAP Audio metadata | Album, title, comment; chapter TIMEBASE/START/END/title/url; `-c:v copy -disposition:v attached_pic` | ID3v2/MP4 metadata and chapters, cover-art packet copying. Per-chapter images currently rejected; no new feature added |
| EnCAP `crates/encap-transcript/src/lib.rs` (two preparation paths) | `-i source -vn -ar 16000 -ac 1 -c:a pcm_s16le input.wav` | Audio decode, mono/16k resampling and PCM WAV. Caption text/SRT export is handled by application code, not FFmpeg subtitle filters |
| EnCAP `crates/encap-core/src/source.rs` and waveform code | Native WAV/AIFF parsing; no FFprobe for source inspection | WAV PCM/IEEE float, AIFF/AIFC; don't add duplicate probing logic to Core solely for this migration |
| ATIV and EnCAP tests | ffprobe `-show_streams -show_format -show_chapters -of json`, stream selection, count_frames in benchmark scripts | codec_name/type/tag, dimensions, pix_fmt, sample_rate/channels/layout, duration/start_time, r_frame_rate/avg_frame_rate, nb_frames/nb_read_frames, tags, chapters, dispositions as used by individual tests |
| Core real-media tests | lavfi sine/color/testsrc2, PNG, PCM fixtures, frame extraction, rawvideo/framemd5/null outputs | Synthetic fixture source filters/device and test muxers/encoders; not new user output formats |

### Accepted inputs cannot be inferred from output encoders alone

ATIV macOS image picker explicitly allows PNG/JPEG/WebP/BMP/TIFF; audio picker WAV/MP3/M4A/AAC/FLAC/Ogg/Opus. Windows drag/drop accepts MIME audio plus WAV/MP3/M4A/M4B/AAC/FLAC/Ogg/OGA/Opus/AIFF/WMA/ALAC extensions. Core and CLI accept existing file paths without restricting all formats to those lists. EnCAP uses native WAV/AIFF/AIFC import and broad macOS `.image` artwork; its Windows artwork picker lists PNG/JPEG/WebP. Preserve native input decoding rather than remove valid input formats based on a short UI list.

The minimum manifest therefore asserts WAV/AIFF/MP3/AAC/MOV-family/FLAC/Ogg/ASF/image and ffmetadata demuxers; MP4/MOV/M4A/MP3/WAV/AIFF/image muxers; PCM signed integer/float LE/BE, AAC/ALAC/MP3/FLAC/Vorbis/Opus/WMA/H.264/HEVC and image decoders. Native internal components beyond this floor remain enabled for input compatibility. Required parser identifiers and automatic AAC extradata conversion/extraction bitstream filters are tracked. No command explicitly requests a bitstream filter; the AAC/MOV path may select one internally. `-parsers` is not a supported FFmpeg CLI query; the initial baseline attempt is preserved as evidence of that limitation, and source builds validate parsers from generated config.

### Hardware strategy remains unchanged

Core's advertised-priority order is VideoToolbox, NVENC, QSV, AMF, VAAPI for both H.264 and HEVC. Automatic encoding retries in software after a hardware process failure; explicit hardware does not silently fall back. ATIV remains software-only by policy. EnCAP exposes automatic/hardware/software. No device arguments or new accelerator defaults are introduced here. Baseline macOS advertises VideoToolbox; AudioToolbox is separately used for EnCAP AAC. Other target inventories and device behavior must be verified before choosing their new runtime configuration.

## Existing binary evidence

The available ATIV ARM64 pair and current EnCAP ARM64 installed pair have matching SHA-256, identify as the Martin Riedl distribution, and advertise libx264/libx265/libmp3lame/AAC, VideoToolbox and AudioToolbox. Their configuration enables many unrelated libraries: libxml2, OpenSSL, font/text/rendering libraries, Blu-ray, streaming, AV1/VPx/VVC, metrics and other codecs. Those are **present**, not requirements demonstrated by the application commands. The new build does not import this dependency tree wholesale.

`baseline/{ativ-arm64,encap-arm64,encap-x86_64}` contain raw version/buildconf/capability inventories. ARM64 records include linkage; the complete location list identifies additional bundled copies and the legacy installed Intel pair. Baseline hardware advertisement is not proof of a working device. The legacy EnCAP Intel install identifies as a custom 9.0.1 build with LAME/AudioToolbox/VideoToolbox and no libx264/libx265; it is an old cached build, not evidence that the current unified fetch recipe lacks those encoders. No Linux or Windows baseline executable was locally runnable in this audit; their release scripts establish acquisition/target support but not verified GPU feature parity.

`baseline/behavior-macos-arm64.json` records the known-working behavioral smoke suite. Candidate validation exercises identical assertions plus exact source identity and linkage. The five existing Core integration tests additionally validate composed pixels, flips, original audio sample format, timing, fractional hard cuts, cadence, HEVC hvc1 and progress. A matching smoke report is functional evidence for those cases, not proof of bit-identical lossy encodes or exhaustive input compatibility.
