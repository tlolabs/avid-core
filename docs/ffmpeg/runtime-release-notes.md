AVID Core FFmpeg 9.0.1, Recipe 7, matched ffmpeg/ffprobe runtime.

This release promotes the exact binaries built at 25d19098a22936638b0e2a70616083d929fe409c by native run 36681249099. The promotion revision is separate and recorded in manifest.json. All six runtime/source pairs must qualify before publication. The signed-tag promotion workflow attests the promotion; it does not claim to have originally built these binaries.

Actual qualification environments were macOS 15.7.9 ARM64/x86_64, Windows 11 build 26200 ARM64, Windows Server 2025 build 26100 x86_64, and Ubuntu 24.04.5/glibc 2.39 ARM64/x86_64. macOS 13 and Windows 10 desktop versions remain untested. Compiler deployment settings are not runtime evidence.

Original SOURCE.json metadata anticipated the earlier ffmpeg-9.0.1-r7 source tag. Those original bytes are preserved as build evidence. The authenticated release manifest provides the authoritative asset mapping for ffmpeg-9.0.1-r7.1.
