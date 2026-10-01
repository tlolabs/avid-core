# Core FFmpeg runtime specification

`spec.json` is the Core-owned Recipe 7 build contract. It pins FFmpeg 9.0.1,
its signed source revision and archive checksum, dependency sources, configure
flags and six native targets. Both `ffmpeg` and `ffprobe` are enabled and built
in one source tree. The Rust crate embeds this specification and rejects a
different package. `qualification.json` records target and host release gates;
candidate builds never change those gates automatically.
