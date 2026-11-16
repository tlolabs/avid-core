#!/usr/bin/env python3
"""Deliberate offline build of the pinned, CPU-only linked Whisper runtime.

Never called by production libraries. Supply the source archive explicitly;
application execution neither downloads nor compiles executable code.
"""
import argparse
import hashlib
import json
import pathlib
import platform
import subprocess
import tarfile
import sys
import shutil
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "scripts/linked"))
from runtime import android_runtime, cxx_runtime

REVISION = "4979e04f5dcaccb36057e059bbaed8a2f5288315"
SOURCE_SHA256 = "ec8e33f4c838ed459ad0a23da255c0262f150a6f54839a17e504ddf1495305d5"
SOURCE_URL = f"https://github.com/ggml-org/whisper.cpp/archive/{REVISION}.tar.gz"
ROOT = pathlib.Path(__file__).resolve().parents[1]
PATCH_DIR = ROOT / "licenses/linked-inference/patches"
PATCH_SPEC = json.loads((PATCH_DIR / "manifest.json").read_text())
TARGETS = {
    "aarch64-apple-darwin": ("Darwin", "arm64"),
    "x86_64-apple-darwin": ("Darwin", "x86_64"),
    "aarch64-apple-ios": ("iOS", "arm64"),
    "aarch64-apple-ios-sim": ("iOS", "arm64"),
    "x86_64-apple-ios": ("iOS", "x86_64"),
    "x86_64-pc-windows-msvc": ("Windows", "AMD64"),
    "aarch64-pc-windows-msvc": ("Windows", "ARM64"),
    "x86_64-pc-windows-gnu": ("Windows", "AMD64"),
    "aarch64-pc-windows-gnullvm": ("Windows", "ARM64"),
    "x86_64-unknown-linux-gnu": ("Linux", "x86_64"),
    "aarch64-unknown-linux-gnu": ("Linux", "aarch64"),
    "aarch64-linux-android": ("Android", "aarch64"),
    "x86_64-linux-android": ("Android", "x86_64"),
}


def sha(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def build(args):
    archive = pathlib.Path(args.source_archive).resolve()
    if sha(archive) != SOURCE_SHA256:
        raise ValueError("whisper.cpp source archive does not match immutable pin")
    build_root = pathlib.Path(args.build).resolve()
    prefix = pathlib.Path(args.prefix).resolve()
    build_root.mkdir(parents=True, exist_ok=True)
    source = build_root / f"whisper.cpp-{REVISION}"
    if source.exists():
        raise ValueError("use a fresh build directory; stale or modified sources are not accepted")
    with tarfile.open(archive) as stream:
        stream.extractall(build_root, filter="data")
    patch = PATCH_DIR / PATCH_SPEC['patch']
    if sha(patch) != PATCH_SPEC['patch_sha256'] or PATCH_SPEC['base_archive_sha256'] != SOURCE_SHA256:
        raise ValueError("Whisper exception-safety patch identity mismatch")
    for name, hashes in PATCH_SPEC['files'].items():
        if sha(source / name) != hashes['base_sha256']:
            raise ValueError("Whisper base file differs before patch: " + name)
    subprocess.run(['git', 'apply', '--check', str(patch)], cwd=source, check=True)
    subprocess.run(['git', 'apply', str(patch)], cwd=source, check=True)
    for name, hashes in PATCH_SPEC['files'].items():
        if sha(source / name) != hashes['patched_sha256']:
            raise ValueError("Whisper patched file identity mismatch: " + name)
    system, arch = TARGETS[args.target]
    platform_name = 'android' if system == 'Android' else ('windows' if system == 'Windows' else ('macos' if system == 'Darwin' else ('ios-simulator' if 'sim' in args.target or (system == 'iOS' and arch == 'x86_64') else 'ios' if system == 'iOS' else 'linux')))
    runtime_target = {'platform': platform_name, 'arch': 'arm64' if arch in ('arm64', 'aarch64', 'ARM64') else 'x86_64'}
    cpp_runtime = None if 'msvc' in args.target else cxx_runtime(runtime_target, args.cxx_runtime)
    options = [
        "-DCMAKE_BUILD_TYPE=Release", f"-DCMAKE_INSTALL_PREFIX={prefix}",
        "-DCMAKE_EXPORT_COMPILE_COMMANDS=ON", "-DBUILD_SHARED_LIBS=OFF",
        "-DWHISPER_BUILD_TESTS=OFF", "-DWHISPER_BUILD_EXAMPLES=OFF",
        "-DWHISPER_BUILD_SERVER=OFF", "-DGGML_METAL=OFF", "-DGGML_BLAS=OFF",
        "-DGGML_NATIVE=OFF", "-DGGML_OPENMP=OFF", "-DGGML_CCACHE=OFF",
        f"-DCMAKE_SYSTEM_NAME={system}", f"-DCMAKE_SYSTEM_PROCESSOR={arch}",
    ]
    if system in ("Darwin", "iOS"):
        sdk = "macosx" if system == "Darwin" else ("iphonesimulator" if "sim" in args.target or arch == "x86_64" else "iphoneos")
        options += [f"-DCMAKE_OSX_ARCHITECTURES={arch}", f"-DCMAKE_OSX_SYSROOT={sdk}",
                    f"-DCMAKE_OSX_DEPLOYMENT_TARGET={args.deployment or ('14.0' if system == 'Darwin' else '17.0')}"]
    elif system == "Android":
        if not args.toolchain or not args.ndk:
            raise ValueError("Android requires --ndk and its pinned NDK CMake toolchain explicitly")
        if pathlib.Path(args.toolchain).resolve() != pathlib.Path(args.ndk).resolve() / 'build/cmake/android.toolchain.cmake':
            raise ValueError("Android toolchain must belong to the selected pinned NDK")
        options += [f"-DANDROID_ABI={'arm64-v8a' if arch == 'aarch64' else 'x86_64'}",
                    "-DANDROID_PLATFORM=android-28", "-DANDROID_STL=c++_shared", "-DANDROID_SUPPORT_FLEXIBLE_PAGE_SIZES=ON",
                    "-DCMAKE_SHARED_LINKER_FLAGS=-Wl,-z,max-page-size=16384"]
    elif not args.toolchain:
        aliases = {"arm64": "aarch64", "ARM64": "aarch64", "AMD64": "x86_64", "amd64": "x86_64"}
        host_arch = aliases.get(platform.machine(), platform.machine())
        target_arch = aliases.get(arch, arch)
        if system != platform.system() or target_arch != host_arch or (system == "Windows" and "msvc" not in args.target):
            raise ValueError("cross-target/GNU Windows builds require an explicit target CMake toolchain")
    if args.toolchain:
        options.append(f"-DCMAKE_TOOLCHAIN_FILE={pathlib.Path(args.toolchain).resolve()}")
    if system == "Windows" and "msvc" in args.target:
        options += ["-A", "ARM64" if arch == "ARM64" else "x64"]
    if system == 'Windows' and 'msvc' not in args.target:
        options += ['-DCMAKE_CXX_FLAGS=-stdlib=' + ('libc++' if cpp_runtime == 'c++' else 'libstdc++')]
    binary = build_root / "objects"
    subprocess.run(["cmake", "-S", str(source), "-B", str(binary), *options], check=True)
    subprocess.run(["cmake", "--build", str(binary), "--config", "Release", "--target", "install", "--parallel", str(args.jobs)], check=True)
    # Record the actual compiler/SDK/target cache, not just intended command-line flags.
    cache = (binary / "CMakeCache.txt").read_text()
    cmake_identity = [line for line in cache.splitlines() if line and not line.startswith(("#", "//")) and any(key in line for key in ("COMPILER", "SYSROOT", "OSX_", "ANDROID", "SYSTEM_", "MSVC", "CMAKE_BUILD_TYPE"))]
    files = [file for file in (prefix / "lib").iterdir() if file.suffix in (".a", ".lib")]
    runtime_dependencies = android_runtime(pathlib.Path(args.ndk), runtime_target, prefix) if system == 'Android' else {}
    manifest = {"schema": 1, "kind": "avid-linked-inference", "runtime": "whisper.cpp-1.8.2",
                "source_revision": REVISION, "source_archive_sha256": SOURCE_SHA256,
                "source_url": SOURCE_URL, "target": args.target, "profile": "cpu-offline",
                "sdk_variant": PATCH_SPEC["variant"], "source_patches": [PATCH_SPEC],
                "cmake": subprocess.check_output(["cmake", "--version"], text=True).splitlines()[0],
                "configuration": options, "compiler_sdk_cache": cmake_identity,
                "libraries": {file.name: sha(file) for file in files},
                "cxx_runtime": cpp_runtime, "runtime_dependencies": runtime_dependencies,
                "sdk_license": "MIT AND GPL-3.0-or-later", "model_licenses": "separate manifests; no model bundled",
                "qualification": {"native_inference": "not_run", "physical_device": "not_run"}}
    (prefix / "avid-whisper-build.json").write_text(json.dumps(manifest, indent=2) + "\n")
    (prefix / "licenses").mkdir(exist_ok=True)
    (prefix / "licenses/whisper-MIT.txt").write_bytes((source / "LICENSE").read_bytes())
    shutil.copy2(ROOT / 'LICENSE', prefix / 'licenses/AVID-EXCEPTION-PATCH-GPL-3.0.txt')
    shutil.copy2(patch, prefix / 'licenses' / patch.name)
    (prefix / 'licenses/patch-manifest.json').write_text(json.dumps(PATCH_SPEC, indent=2) + "\n")
    print(prefix / "avid-whisper-build.json")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-archive", required=True)
    parser.add_argument("--build", required=True)
    parser.add_argument("--prefix", required=True)
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument("--toolchain")
    parser.add_argument("--ndk")
    parser.add_argument("--cxx-runtime", choices=["c++", "stdc++", "c++_shared"])
    parser.add_argument("--deployment")
    parser.add_argument("--jobs", type=int, default=4)
    build(parser.parse_args())
