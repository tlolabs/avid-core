# Android linked test host

Requires JDK17, Gradle8.11.1, Android SDK35, NDK 28.2.13676358 and verified Core Android
API26+ `arm64-v8a` / `x86_64` artifacts. This host has not been compiled or run in
the current environment, which has no Android SDK/NDK/emulator/device.

Build Core/FFmpeg with the target instructions in `docs/linked-artifacts.md`.
Build this transport separately for each ABI:

```sh
cmake -S . -B build-arm64 -DCMAKE_TOOLCHAIN_FILE="$ANDROID_NDK_HOME/build/cmake/android.toolchain.cmake" -DANDROID_ABI=arm64-v8a -DANDROID_PLATFORM=android-26 -DAVID_ARTIFACT_DIR=/absolute/verified/core-arm64
cmake --build build-arm64
```

Put `libavid_core_jni.so`, `libavid_core_ffi.so` and every listed transitive
non-system `.so` in `jniLibs/arm64-v8a/`; repeat for x86_64. Inspect every ELF
PT_LOAD alignment and APK ZIP alignment for 16 KB page compatibility; see Core's
packaging verifier. Use `gradle assembleDebug` then install the development APK
with adb on a 16 KB emulator/device. The host requires no Internet permission.
It generates PNG/WAV, probes, previews, and exports both compositions and modes
(single inputs and timelines), receives progress, and cancels another export
while preserving an existing destination. Independent Android MediaCodec decoders
check geometry, decoded red artwork, frame counts, PCM tone/sample counts, audio
layout and timestamps. Decode queues retain one buffer at a time and have a
30-second diagnostic deadline; Core's cooperative deadline contract is unchanged.
Run and retrieve its machine-readable report without screen interaction:

```sh
adb shell am start -W -n org.avidcore.linkedtest/org.avidcore.MainActivity
# Wait for AVIDLinkedHost to report completion, then retrieve the final JSON.
adb logcat -s AVIDLinkedHost:I
adb shell run-as org.avidcore.linkedtest cat files/result.json
```

Require `status: passed`, four exports with actual decoded frame/audio counts,
progress, cancellation and destination preservation. A stale report is removed
when the Activity starts. Record device/emulator, Android/ABI and actual page size
(`adb shell getconf PAGE_SIZE`) with the result. Compilation, normal-page execution
and 16 KB execution are separate checks; playback/OS interruption and long-input
memory still require each target's native qualification.
No release/store upload is configured.

`LinkedCore` is a thin synchronous wrapper. Hosts must import content URIs through
ContentResolver and pass sandbox-local files. Do not block the UI thread. Cancel
on interruption and wait for worker completion before unloading libraries.
The wrapper checks ABI 1, closes idempotently, and cancels native work when a
Java event callback throws. The original exception propagates only after Core
has destroyed its operation contexts. Cancellation before publication preserves
the destination. An exception from a `complete` event occurs after successful
publication and cannot roll it back.

## Desktop JNI transport regression

The same JNI C transport and Java wrapper can be tested with a native macOS/Linux
Core shared library and a local JDK 17+. This exercises actual export, synchronous
progress, Java callback exceptions, cooperative cancellation, staging cleanup,
destination preservation and a subsequent successful export, with no media
executables on PATH. It does **not** compile the Android Activity/MediaCodec host
or establish Android/NDK/16 KB runtime qualification.

From the repository root, build Core with an explicit verified FFmpeg prefix as
described in `docs/linked-artifacts.md`, then run:

```sh
python3 examples/android-linked/test_jni.py \
  --java-home /absolute/JDK/Contents/Home \
  --core-library "$PWD/target/debug/libavid_core_ffi.dylib" \
  --output /tmp/avid-desktop-jni-test
```

On Linux, supply the native `.so` and Linux JDK home instead. The output directory
must be new. Compiler/JVM commands and individual logs remain in that directory;
the resulting test MP4 is under `work/` for separate independent decoding.
These are development/test artifacts, not notarized macOS app distributions.
