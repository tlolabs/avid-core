# Privacy

AVID Core is a local source library. It contains no analytics, telemetry, crash reporter, update checker, remote font or asset loader, or network client. Its media operations invoke caller-supplied FFmpeg and ffprobe paths and restrict media input protocols to local `file` and `pipe`. The core feature works offline when the host provides those tools and local media.

The library has no persistent log store or log retention policy. It returns progress and diagnostic events to the consuming application; those diagnostics can include user-selected local paths. ATIV and EnCAP decide whether, where, and how long to keep local logs and must document their own behavior. Their update checks, downloads, model services, application telemetry, and bundled runtime behavior are outside AVID Core's privacy boundary.

Development and CI may access Cargo registries or download test/build tools. Those activities are not AVID Core application runtime behavior.
