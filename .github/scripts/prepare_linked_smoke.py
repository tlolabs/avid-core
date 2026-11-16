#!/usr/bin/env python3
"""Prepare only native linked build inputs for the existing weekly smoke job.

This developer/CI helper never packages or publishes a runtime or runs a full
qualification suite. Older source tags retain their original compilation path.
"""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import subprocess
import tarfile
import zipfile

ROOT = Path(__file__).resolve().parents[2]
TARGETS = {
    'linux-x64': ('linux-x86_64', 'x86_64-unknown-linux-gnu'),
    'linux-arm64': ('linux-arm64', 'aarch64-unknown-linux-gnu'),
    'windows-x64': ('windows-x86_64', 'x86_64-pc-windows-gnu'),
    'windows-arm64': ('windows-arm64', 'aarch64-pc-windows-gnullvm'),
}


def release_layout(source, target, legacy_target):
    if target not in TARGETS:
        raise ValueError('Weekly smoke supports only its four native Linux/Windows targets')
    linked = (source / 'crates/avid-media-bridge/build.rs').is_file()
    return {'linked': linked, 'rust_target': TARGETS[target][1] if linked else legacy_target}


def prepare(source, target, work):
    target_id, _ = TARGETS[target]
    # Use the current controller's unchanged pinned tool archive identity, not an
    # arbitrary executable URL supplied by a source tag.
    spec = json.loads((ROOT / 'runtime/ffmpeg/spec.json').read_text())
    cmake_key = 'windows-' + ('arm64' if target.endswith('arm64') else 'x86_64') if target.startswith('windows') else target_id
    item = spec['build_tools']['cmake_archives'][cmake_key]
    work.mkdir(parents=True, exist_ok=False)
    archive = work / item['url'].rsplit('/', 1)[1]
    subprocess.run(['curl', '--fail', '--location', '--proto', '=https', '--proto-redir', '=https',
                    '--output', str(archive), item['url']], check=True)
    with archive.open('rb') as stream:
        if hashlib.file_digest(stream, 'sha256').hexdigest() != item['sha256']:
            raise ValueError('Pinned CMake archive integrity mismatch')
    tools = work / 'tools'; tools.mkdir()
    if archive.suffix == '.zip':
        with zipfile.ZipFile(archive) as stream:
            for name in stream.namelist():
                if Path(name).is_absolute() or '..' in Path(name).parts:
                    raise ValueError('Unsafe CMake archive path')
            stream.extractall(tools)
    else:
        with tarfile.open(archive) as stream:
            stream.extractall(tools, filter='data')
    executables = list(tools.glob('*/bin/cmake.exe' if target.startswith('windows') else '*/bin/cmake'))
    if len(executables) != 1:
        raise ValueError('Pinned CMake archive has unexpected layout')
    sdk = work / 'sdk'
    command = ['python3', str(source / 'scripts/linked/build.py'), '--target', target_id,
               '--profile', 'gpl-software', '--work', str(sdk), '--cache', str(work / 'sources'),
               '--cmake', str(executables[0]), '--jobs', '4', '--fetch']
    if target.startswith('windows'):
        # CLANG64/CLANGARM64 explicitly select libc++, including the x86 GNU
        # Rust C ABI target. No MSVC object or C++ ABI is mixed into the SDK.
        command += ['--cc', 'clang', '--cxx', 'clang++', '--cxx-runtime', 'c++']
    subprocess.run(command, cwd=source, check=True)
    return sdk / 'prefix', executables[0].parent


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--target', choices=TARGETS, required=True)
    parser.add_argument('--work', type=Path)
    parser.add_argument('--detect', action='store_true')
    parser.add_argument('--legacy-rust-target')
    args = parser.parse_args()
    if args.detect:
        layout = release_layout(args.source.resolve(), args.target, args.legacy_rust_target)
        print('linked=' + str(layout['linked']).lower())
        print('rust_target=' + layout['rust_target'])
    else:
        if not args.work:
            parser.error('--work is required when preparing SDK inputs')
        prefix, tools = prepare(args.source.resolve(), args.target, args.work.resolve())
        print('Linked SDK prefix:', prefix)
        print('Pinned build tools:', tools)
