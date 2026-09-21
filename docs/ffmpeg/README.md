# FFmpeg research archive

Core 0.3.0 is a Rust source library. Hosts supply and qualify FFmpeg/ffprobe.
Nothing in this directory or `runtime/ffmpeg/` is a Core source-release gate.
Use [the integration contract](../integration.md) for current instructions.

The earlier source-build investigation remains useful for compiler regressions,
Windows handle lifetime, software encoding, reproducible builds, licensing
research and platform diagnostics. The pinned recipes and qualification JSON
are historical observations, not supported runtime mappings or shipping approval.

- `scripts/ffmpeg/build.py`, `validate.py`, `repeat.py`, `hardware_probe.py` and
  their helper modules remain diagnostic tools. Run `bash scripts/ffmpeg/ci.sh
  <target>` only for an intentional native research build. It does not publish.
- `.github/workflows/ffmpeg.yml` is manual research and uploads diagnostic JSON
  only. Other investigation workflows are also manual; old run artifacts may
  have expired. They are not prerequisites for library tags or host releases.
- `historical/*.py.txt` and `historical/ffmpeg-release.yml.txt` preserve the removed
  acquisition, staging, promotion, release-manifest, archive-qualification and
  publishing implementation for reference. They are not executable entry points.
- Historical reports, baseline captures, evidence hashes and compiler fixtures
  are retained. Read their conclusions in the context of the recorded binaries.

The default Core tests need no FFmpeg. Optional real-media and handle-release
checks accept caller-supplied binaries; see [verification](../integration.md#verification).
