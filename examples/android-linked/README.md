# Android linked test host

Requires JDK17, Gradle8.11.1, Android SDK35, NDK r28+ and verified Core Android
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
with adb on a 16 KB emulator/device. Run `org.avidcore.linkedtest` and inspect
its result (the host requires no Internet permission). It generates PNG/WAV,
exports through JNI, receives progress, cancels another export and checks
preservation and MP4 tracks. MP4 track inspection alone is not decoded playback
qualification: validate playback, decoded frames/audio and memory on each device.
No release/store upload is configured.

`LinkedCore` is a thin synchronous wrapper. Hosts must import content URIs through
ContentResolver and pass sandbox-local files. Do not block the UI thread. Cancel
on interruption and wait for worker completion before unloading libraries.
