# Core FFmpeg runtime specification

`spec.json` is the Core-owned Recipe 7 build contract. It pins FFmpeg 9.0.1,
its signed source revision and archive checksum, dependency sources, configure
flags and six native targets. Both `ffmpeg` and `ffprobe` are enabled and built
in one source tree. The Rust crate embeds this specification and rejects a
different package. `qualification.json` records native evidence and optional
host observations. Candidate builds never change those records automatically.
The embedded specification retains a historical
`qualification_policy.host_packaging` value for compatibility with already
published runtime packages; it is not an active publication gate. See
[the provider contract](../../docs/ffmpeg/provider-contract.md).
