#!/usr/bin/env python3
"""Create an unqualified linked-library candidate with exact source and dependency identities."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tarfile

from binary import validate_file
from runtime import verify_inference_patch, verify_sdk_spec, verified_runtime_dependencies
from rust_runtime import copy_rust_runtime
from source import ROOT, SPEC_PATH, digest, run

SYSTEM_ELF = {'libc.so.6', 'libm.so.6', 'libpthread.so.0', 'libdl.so.2', 'librt.so.1',
              'libgcc_s.so.1', 'libstdc++.so.6', 'libc.so', 'libm.so', 'libdl.so', 'liblog.so', 'libandroid.so'}
SYSTEM_ANDROID = {'libc.so', 'libm.so', 'libdl.so', 'liblog.so', 'libandroid.so', 'libz.so'}
SYSTEM_WINDOWS = {'kernel32.dll', 'ntdll.dll', 'user32.dll', 'advapi32.dll', 'bcrypt.dll', 'ws2_32.dll',
                  'ole32.dll', 'shell32.dll', 'userenv.dll', 'secur32.dll', 'normaliz.dll', 'msvcrt.dll',
                  'ucrtbase.dll', 'vcruntime140.dll', 'vcruntime140_1.dll'}


def payload_files(directory):
    files = []
    for path in sorted(directory.rglob('*')):
        if path.is_symlink():
            raise ValueError('Package symlinks are not supported')
        if path.is_file():
            files.append(path)
    return files


def core_sources():
    # Include exact current source, including new nonignored bridge/host files.
    names = subprocess.check_output(['git', '-C', str(ROOT), '-c', 'core.fsmonitor=false',
                                     'ls-files', '-co', '--exclude-standard', '-z']).decode().split('\0')
    roots = ('src/', 'crates/', 'include/', 'scripts/linked/', 'runtime/linked/', 'runtime/ffmpeg/', 'examples/', 'licenses/', 'tests/', 'apple/')
    fixed = {'Cargo.toml', 'Cargo.lock', 'LICENSE', 'README.md', 'THIRD_PARTY_NOTICES.md', 'rust-toolchain.toml',
             'LICENSING_REVIEW.md', 'docs/BUILDING.md', 'docs/DEPENDENCIES.md',
             'docs/dependency-inventory.json', 'script/build_whisper.py'}
    return sorted({name for name in names if name and (name in fixed or name.startswith(roots) or (name.startswith('docs/linked') and name.endswith('.md')))
                   and (ROOT / name).is_file() and not (ROOT / name).is_symlink()})


def source_identity(names):
    files = {name: digest(ROOT / name) for name in names}
    return {'files': files, 'sha256': hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest(),
            'head': run(['git', '-C', ROOT, 'rev-parse', 'HEAD'])}



def inference_sdk(prefix, target):
    prefix = Path(prefix).resolve()
    path = prefix / 'avid-whisper-build.json'
    metadata = json.loads(path.read_text())
    if (metadata.get('schema') != 1 or metadata.get('kind') != 'avid-linked-inference'
        or metadata.get('runtime') != 'whisper.cpp-1.8.2'
        or metadata.get('source_revision') != '4979e04f5dcaccb36057e059bbaed8a2f5288315'
        or metadata.get('source_archive_sha256') != 'ec8e33f4c838ed459ad0a23da255c0262f150a6f54839a17e504ddf1495305d5'
        or metadata.get('profile') != 'cpu-offline' or metadata.get('target') != target['rust_target']):
        raise ValueError('Inference prefix does not match pinned runtime/target')
    verify_inference_patch(prefix, metadata)
    libraries = metadata.get('libraries', {})
    required = {'lib' + name + '.a' for name in ('whisper', 'ggml', 'ggml-base', 'ggml-cpu')}
    if set(libraries) != required:
        raise ValueError('Unexpected inference native library inventory')
    for name, expected in libraries.items():
        path = prefix / 'lib' / name
        if not path.resolve().is_relative_to(prefix) or digest(path) != expected:
            raise ValueError('Inference library integrity mismatch: ' + name)
        validate_file(path, target)
    if not (prefix / 'licenses/whisper-MIT.txt').is_file():
        raise ValueError('Missing inference SDK license notices')
    return {'metadata_sha256': digest(prefix / 'avid-whisper-build.json'), 'metadata': metadata}


def runtime_minimum(target, inference):
    media = target['minimum']
    value = None
    if inference:
        for option in inference['metadata'].get('configuration', []):
            if option.startswith('-DCMAKE_OSX_DEPLOYMENT_TARGET='):
                value = option.split('=', 1)[1]
            elif option.startswith('-DANDROID_PLATFORM=android-'):
                value = option.removeprefix('-DANDROID_PLATFORM=android-')
    combined = media
    if value is not None and target['platform'] in ('macos', 'ios', 'ios-simulator', 'android'):
        version = lambda text: tuple(int(component) for component in text.split('.'))
        if version(value) > version(media):
            combined = value
    return {'media': media, 'inference': value, 'combined': combined}

def pe_dependencies(path):
    output = run(['llvm-readobj', '--coff-imports', path])
    return sorted(set(re.findall(r'^\s+Name: (\S+)\s*$', output, re.M)))


def write_archive(directory, path, epoch):
    def normalized(info):
        info.uid = info.gid = 0
        info.uname = info.gname = ''
        info.mtime = epoch
        return info
    with path.open('wb') as raw, gzip.GzipFile(fileobj=raw, filename='', mode='wb', mtime=epoch) as compressed:
        with tarfile.open(fileobj=compressed, mode='w') as archive:
            for file in payload_files(directory):
                archive.add(file, arcname=directory.name + '/' + file.relative_to(directory).as_posix(), filter=normalized)


def package(args):
    spec = json.loads(SPEC_PATH.read_text())
    target = next(t for t in spec['targets'] if t['id'] == args.target)
    prefix = args.prefix.resolve()
    metadata = json.loads((prefix / 'linked-ffmpeg.json').read_text())
    if metadata.get('schema') != 1 or metadata.get('kind') != 'avid-linked-ffmpeg' or metadata.get('target') != target['rust_target']:
        raise ValueError('FFmpeg prefix identity differs from target')
    if metadata.get('source_revision') != spec['ffmpeg']['revision']:
        raise ValueError('FFmpeg prefix source/specification identity differs')
    verify_sdk_spec(prefix, metadata, spec)
    if metadata.get('source_provenance', {}).get('status') != 'verified':
        raise ValueError('FFmpeg source has not been authenticated')
    for name, expected in metadata['libraries'].items():
        if digest(prefix / name) != expected:
            raise ValueError('FFmpeg library differs from recorded build: ' + name)
        validate_file(prefix / name, target)
    core_library = args.core_library.resolve()
    core_binary = validate_file(core_library, target)
    build_receipt = json.loads(args.core_build_receipt.read_text())
    if build_receipt.get('kind') != 'avid-linked-core-build' or build_receipt.get('target') != target['rust_target']:
        raise ValueError('Core build receipt target differs')
    if build_receipt.get('ffmpeg_prefix_metadata_sha256') != digest(prefix / 'linked-ffmpeg.json'):
        raise ValueError('Core was built against another FFmpeg prefix')
    if build_receipt.get('binaries', {}).get(core_library.name, {}).get('sha256') != digest(core_library):
        raise ValueError('Core library differs from fixed-source build receipt')
    for name, item in metadata['sources'].items():
        if digest(prefix / 'corresponding-source' / item['archive']) != item['sha256']:
            raise ValueError('Corresponding source archive differs from build: ' + name)
    inference = None
    if 'whisper' in build_receipt.get('features', []):
        if not args.whisper_prefix or not args.whisper_source_archive:
            raise ValueError('Whisper packaging requires --whisper-prefix and --whisper-source-archive for exact source/notices')
        inference = inference_sdk(args.whisper_prefix, target)
        if inference != build_receipt.get('inference'):
            raise ValueError('Inference SDK differs from Core build receipt')
        if digest(args.whisper_source_archive) != inference['metadata']['source_archive_sha256']:
            raise ValueError('Whisper corresponding source archive differs from immutable pin')
    elif args.whisper_prefix or args.whisper_source_archive:
        raise ValueError('Inference source supplied for a Core build without whisper support')
    provided_runtime = {path.name: path.resolve() for path in args.runtime_dependency}
    if len(provided_runtime) != len(args.runtime_dependency):
        raise ValueError('Duplicate runtime dependency filenames')
    runtime_files = verified_runtime_dependencies(prefix, metadata, target)
    if inference:
        inference_runtime = verified_runtime_dependencies(args.whisper_prefix, inference['metadata'], target)
        for name, path in inference_runtime.items():
            if name in runtime_files and digest(runtime_files[name]) != digest(path):
                raise ValueError('Media and inference SDKs use different C++ runtimes')
            runtime_files.setdefault(name, path)
    for name, path in provided_runtime.items():
        if name in runtime_files and digest(runtime_files[name]) != digest(path):
            raise ValueError('Runtime override differs from pinned SDK runtime')
        runtime_files[name] = path
    supplied = set(runtime_files)
    binaries = {core_library.name: core_binary}
    for name, path in runtime_files.items():
        binaries[name] = validate_file(path, target)
    for name, info in binaries.items():
        dependencies = info.get('dependencies', [])
        if target['platform'] == 'windows' and Path(name).suffix.lower() == '.dll':
            dependencies = pe_dependencies(core_library if name == core_library.name else runtime_files[name])
            info['dependencies'] = dependencies
        for dependency in dependencies:
            basename = Path(dependency).name
            if target['platform'] == 'windows':
                if basename.lower() in SYSTEM_WINDOWS or basename.lower().startswith(('api-ms-win-', 'ext-ms-win-')):
                    continue
                found = basename.lower() in {value.lower() for value in supplied}
            elif target['platform'] in ('linux', 'android'):
                if basename in (SYSTEM_ANDROID if target['platform'] == 'android' else SYSTEM_ELF):
                    continue
                found = basename in supplied
            else:
                if dependency.startswith(('/System/Library/', '/usr/lib/')):
                    continue
                found = basename in supplied
            if not found:
                raise ValueError('Missing transitive runtime dependency: ' + dependency)
    names = core_sources()
    before = source_identity(names)
    if build_receipt['core_source'] != before:
        raise ValueError('Current source differs from Core build receipt; rebuild before packaging')
    args.output.mkdir(parents=True, exist_ok=True)
    artifact_id = 'avid-linked-v' + spec['artifact_version'] + '-' + metadata['profile'] + ('-whisper' if inference else '') + '-' + args.target
    destination = args.output.resolve() / artifact_id
    destination.mkdir(exist_ok=False)
    (destination / 'lib').mkdir()
    (destination / 'include').mkdir()
    shutil.copy2(core_library, destination / 'lib' / core_library.name)
    for name, path in runtime_files.items():
        shutil.copy2(path, destination / 'lib' / name)
    for name in ('avid_core.h', 'module.modulemap'):
        shutil.copy2(ROOT / 'include' / name, destination / 'include' / name)
    if not build_receipt.get('rust_runtime'):
        raise ValueError('Core receipt lacks Rust std/compiler_builtins runtime notices; rebuild the receipt')
    copy_rust_runtime(build_receipt['rust_runtime'], destination / 'licenses/rust-runtime')
    if (prefix / 'runtime/licenses').is_dir():
        shutil.copytree(prefix / 'runtime/licenses', destination / 'licenses' / 'native-runtime')
    shutil.copytree(prefix / 'licenses', destination / 'licenses' / 'ffmpeg-build')
    shutil.copytree(ROOT / 'licenses/image-optimization', destination / 'licenses' / 'image-optimization')
    for name in ('LICENSE', 'THIRD_PARTY_NOTICES.md'):
        shutil.copy2(ROOT / name, destination / 'licenses' / name)
    if inference:
        shutil.copytree(args.whisper_prefix / 'licenses', destination / 'licenses' / 'whisper')
    corresponding = destination / 'corresponding-source'
    shutil.copytree(prefix / 'corresponding-source', corresponding / 'ffmpeg')
    if inference:
        source = corresponding / 'whisper'
        source.mkdir()
        shutil.copy2(args.whisper_source_archive, source / 'whisper.cpp-pinned.tar.gz')
        for name in ('avid-exception-safety-r1.patch.txt', 'patch-manifest.json', 'AVID-EXCEPTION-PATCH-GPL-3.0.txt'):
            shutil.copy2(args.whisper_prefix / 'licenses' / name, source / name)
        (source / 'avid-whisper-build.json').write_text(json.dumps(inference['metadata'], indent=2) + '\n')
    for name in names:
        path = corresponding / 'avid-core' / name
        path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / name, path)
    # Rust crate sources are content-addressed by Cargo.lock; vendoring is explicit
    # and offline so a package cannot silently acquire new dependencies.
    vendor = corresponding / 'cargo-vendor'
    vendor.mkdir()
    vendor_config = subprocess.check_output(['cargo', 'vendor', '--locked', '--offline', '--versioned-dirs', str(vendor)], cwd=ROOT, text=True)
    (corresponding / 'cargo-vendor-config.toml').write_text(vendor_config.replace(str(vendor), '../cargo-vendor'))
    if before != source_identity(names):
        raise ValueError('Core source changed during packaging; discard candidate')
    record = {'schema': 1, 'kind': 'avid-linked-core-artifact', 'artifact_id': artifact_id, 'abi_version': 1,
              'target': target, 'ffmpeg': metadata, 'core_build': build_receipt, 'core_source': before, 'binaries': binaries,
              'runtime_dependencies': metadata.get('runtime_dependencies', {}), 'inference': inference, 'minimum_runtime': runtime_minimum(target, inference), 'qualification': {'status': 'unqualified', 'native_runtime': 'not_run', 'host_integration': 'not_run'},
              'notarization': 'not_applicable_library_artifact',
              'licenses': {'avid-core': 'GPL-3.0-or-later', 'ffmpeg': metadata['ffmpeg_license'],
                           'native_image_dependencies': spec['native_image_dependencies'],
                           'rust_runtime': build_receipt['rust_runtime'],
                           'whisper.cpp_and_ggml': 'MIT AND GPL-3.0-or-later; AVID patch source included; model licenses separate, no models included' if inference else 'not_linked'},
              'files': {path.relative_to(destination).as_posix(): digest(path) for path in payload_files(destination)}}
    (destination / 'linked-artifact.json').write_text(json.dumps(record, indent=2) + '\n')
    (destination / 'SHA256SUMS').write_text(''.join(digest(path) + '  ' + path.relative_to(destination).as_posix() + '\n'
                                                for path in payload_files(destination) if path.name != 'SHA256SUMS'))
    archive = destination.with_suffix('.tar.gz')
    write_archive(destination, archive, spec['source_date_epoch'])
    archive.with_name(archive.name + '.sha256').write_text(digest(archive) + '  ' + archive.name + '\n')
    print('Packaged unqualified linked candidate:', archive)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target', required=True)
    parser.add_argument('--prefix', type=Path, required=True)
    parser.add_argument('--core-library', type=Path, required=True)
    parser.add_argument('--core-build-receipt', type=Path, required=True)
    parser.add_argument('--whisper-prefix', type=Path)
    parser.add_argument('--whisper-source-archive', type=Path)
    parser.add_argument('--runtime-dependency', type=Path, action='append', default=[])
    parser.add_argument('--output', type=Path, required=True)
    package(parser.parse_args())
