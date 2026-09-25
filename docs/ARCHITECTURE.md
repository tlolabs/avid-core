# Architecture

AVID Core is the shared Rust media engine used by ATIV and EnCAP. Each host owns its UI, application state, updater, logging, FFmpeg/ffprobe supply and platform packaging. Core accepts caller-supplied tool paths and local media, then performs validation, preview and rendering on a caller-managed worker thread.

The [README architecture overview](../README.md#architecture) describes the major APIs. [Integration](integration.md) specifies the host boundary; [provenance](provenance.md) records how the source was reconciled. Historical FFmpeg build research is retained under [docs/ffmpeg](ffmpeg/README.md) and is separate from library runtime distribution.
