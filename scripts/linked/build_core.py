#!/usr/bin/env python3
"""Build Core ABI with a fixed source snapshot and record binary/source/target association."""
import argparse
import json
import os
from pathlib import Path
import subprocess

from binary import validate_file
from package import core_sources, source_identity, inference_sdk, runtime_minimum
from runtime import verify_sdk_spec, verified_runtime_dependencies
from rust_runtime import inspect_rust_runtime
from source import ROOT, SPEC_PATH, digest


def build(args):
    spec = json.loads(SPEC_PATH.read_text())
    target = next(t for t in spec['targets'] if t['id'] == args.target)
    prefix = args.prefix.resolve()
    metadata = json.loads((prefix / 'linked-ffmpeg.json').read_text())
    if metadata.get('target') != target['rust_target']:
        raise ValueError('FFmpeg prefix target/specification differs from requested Core target')
    verify_sdk_spec(prefix, metadata, spec)
    media_runtime = verified_runtime_dependencies(prefix, metadata, target)
    inference = None
    if args.whisper:
        whisper_prefix = os.environ.get('AVID_WHISPER_PREFIX')
        if not whisper_prefix:
            raise ValueError('--whisper requires an explicit AVID_WHISPER_PREFIX')
        inference = inference_sdk(Path(whisper_prefix), target)
        inference_runtime = verified_runtime_dependencies(Path(whisper_prefix), inference['metadata'], target)
        if target['platform'] == 'android' and {name: digest(path) for name, path in media_runtime.items()} != {name: digest(path) for name, path in inference_runtime.items()}:
            raise ValueError('Media and inference SDKs must use the same shared Android C++ runtime')
        if target['platform'] == 'windows' and metadata.get('cxx_runtime') != inference['metadata'].get('cxx_runtime'):
            raise ValueError('Media and inference SDKs must select the same Windows C++ runtime')
    rust_runtime = inspect_rust_runtime(target['rust_target'])
    names = core_sources()
    before = source_identity(names)
    target_dir = args.target_dir.resolve()
    env = dict(os.environ, AVID_FFMPEG_PREFIX=str(prefix), AVID_FFMPEG_STATIC='1',
               CARGO_TARGET_DIR=str(target_dir), SOURCE_DATE_EPOCH=str(spec['source_date_epoch']))
    env.pop('AVID_FFMPEG_DEV', None)
    minimum = runtime_minimum(target, inference)
    if target['platform'] == 'macos':
        env['MACOSX_DEPLOYMENT_TARGET'] = minimum['combined']
    elif target['platform'] in ('ios', 'ios-simulator'):
        env['IPHONEOS_DEPLOYMENT_TARGET'] = minimum['combined']
    command = ['cargo', 'build', '--locked', '--offline', '-p', 'avid-core-ffi', '--release', '--target', target['rust_target']]
    if args.whisper:
        command += ['--features', 'whisper']
    # Cross tools must be installed explicitly; Cargo and cc/cmake receive the
    # target triple and SDK. Android needs NDK CC/AR and linker values from the
    # documented host environment; those are recorded, never guessed from PATH.
    subprocess.run(command, cwd=ROOT, env=env, check=True)
    if args.whisper and inference_sdk(Path(whisper_prefix), target) != inference:
        raise ValueError('Inference SDK changed during Core build')
    if inspect_rust_runtime(target['rust_target']) != rust_runtime:
        raise ValueError('Rust target runtime changed during Core build')
    after_names = core_sources()
    if names != after_names or before != source_identity(after_names):
        raise ValueError('Core source changed during build; no source receipt was issued')
    directory = target_dir / target['rust_target'] / 'release'
    extension = '.lib' if target['rust_target'].endswith('msvc') else '.a'
    static = directory / ('avid_core_ffi' + extension if extension == '.lib' else 'libavid_core_ffi.a')
    shared = directory / ('avid_core_ffi.dll' if target['platform'] == 'windows' else
                           ('libavid_core_ffi.dylib' if 'sdk' in target else 'libavid_core_ffi.so'))
    binaries = {}
    for path in [static, shared]:
        binaries[path.name] = {'sha256': digest(path), 'identity': validate_file(path, target)}
    receipt = {'schema': 1, 'kind': 'avid-linked-core-build', 'target': target['rust_target'],
               'ffmpeg_prefix_metadata_sha256': digest(prefix / 'linked-ffmpeg.json'), 'core_source': before,
               'command': command, 'features': ['whisper'] if args.whisper else [], 'minimum_runtime': minimum, 'inference': inference, 'binaries': binaries,
               'rust': rust_runtime['rustc'], 'rust_runtime': rust_runtime,
               'cross_environment': {key: value for key, value in env.items()
                                     if key.startswith(('CC_', 'CXX_', 'AR_', 'CARGO_TARGET_', 'CFLAGS_', 'CMAKE_'))
                                     and key != 'CARGO_TARGET_DIR'},
               'qualification': 'compiled_and_binary_inspected; native execution not established'}
    args.receipt.parent.mkdir(parents=True, exist_ok=True)
    args.receipt.write_text(json.dumps(receipt, indent=2) + '\n')
    print('Recorded fixed-source Core build:', args.receipt)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target', required=True)
    parser.add_argument('--prefix', required=True, type=Path)
    parser.add_argument('--target-dir', required=True, type=Path)
    parser.add_argument('--receipt', required=True, type=Path)
    parser.add_argument('--whisper', action='store_true')
    build(parser.parse_args())
