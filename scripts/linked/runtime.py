"""Explicit target C++ runtime ownership and linked SDK specification comparison."""
import json
from pathlib import Path
import platform
import shutil

from binary import validate_file
from source import ROOT, digest


def verify_inference_patch(prefix, metadata):
    """Reject the unpatched SDK and retain the exact modified source and rights."""
    directory = ROOT / 'licenses/linked-inference/patches'
    expected = json.loads((directory / 'manifest.json').read_text())
    patch_name = 'avid-exception-safety-r1.patch.txt'
    if (expected.get('variant') != 'avid-exception-safety-r1' or expected.get('patch') != patch_name
        or digest(directory / patch_name) != expected.get('patch_sha256')
        or metadata.get('sdk_variant') != expected['variant']
        or metadata.get('source_patches') != [expected]
        or metadata.get('sdk_license') != 'MIT AND GPL-3.0-or-later'):
        raise ValueError('Inference SDK exception-safety patch identity/license mismatch; rebuild the SDK')
    canonical = prefix.resolve()
    for name, original in ((patch_name, directory / patch_name),
                           ('AVID-EXCEPTION-PATCH-GPL-3.0.txt', ROOT / 'LICENSE'),
                           ('whisper-MIT.txt', ROOT / 'licenses/linked-inference/whisper.cpp-MIT.txt')):
        path = prefix / 'licenses' / name
        if not path.resolve().is_relative_to(canonical) or not path.is_file() or digest(path) != digest(original):
            raise ValueError('Inference SDK missing or changed patch source/license: ' + name)
    manifest = prefix / 'licenses/patch-manifest.json'
    if (not manifest.resolve().is_relative_to(canonical) or not manifest.is_file()
        or json.loads(manifest.read_text()) != expected):
        raise ValueError('Inference SDK patch manifest differs from current source')


def cxx_runtime(target, requested=None):
    if target['platform'] == 'android':
        if requested not in (None, 'c++_shared'):
            raise ValueError('Android linked libraries use one shared NDK libc++ runtime')
        return 'c++_shared'
    if target['platform'] == 'windows':
        if requested not in ('c++', 'stdc++'):
            raise ValueError('Windows GNU/LLVM builds require explicit --cxx-runtime c++ or stdc++')
        return requested
    expected = 'c++' if target['platform'] in ('macos', 'ios', 'ios-simulator') else 'stdc++'
    if requested not in (None, expected):
        raise ValueError('C++ runtime does not match selected toolchain profile')
    return expected


def android_runtime(ndk, target, prefix):
    """Copy the exact pinned NDK redistributable and notices; it is not an OS library."""
    ndk = Path(ndk).resolve()
    revision = '28.2.13676358'
    import re
    if not re.search(r'Pkg.Revision\s*=\s*' + re.escape(revision) + r'\s*$', (ndk / 'source.properties').read_text(), re.M):
        raise ValueError('Android runtime must come from pinned NDK ' + revision)
    host = {'Darwin': 'darwin-x86_64', 'Linux': 'linux-x86_64', 'Windows': 'windows-x86_64'}[platform.system()]
    triple = 'aarch64-linux-android' if target['arch'] == 'arm64' else 'x86_64-linux-android'
    runtime = ndk / 'toolchains/llvm/prebuilt' / host / 'sysroot/usr/lib' / triple / 'libc++_shared.so'
    validate_file(runtime, target)
    notices = [path for path in [ndk / 'sources/cxx-stl/llvm-libc++/NOTICE', ndk / 'NOTICE',
                                ndk / 'toolchains/llvm/prebuilt' / host / 'NOTICE'] if path.is_file()]
    if not notices:
        raise ValueError('Missing NDK libc++ redistribution notices')
    destination = prefix / 'runtime/lib/libc++_shared.so'
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(runtime, destination)
    copied = {}
    for index, notice in enumerate(notices):
        path = prefix / 'runtime/licenses' / ('NDK-NOTICE-' + str(index) + '.txt')
        path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(notice, path)
        copied[path.relative_to(prefix).as_posix()] = digest(path)
    return {'libc++_shared.so': {'path': destination.relative_to(prefix).as_posix(), 'sha256': digest(destination),
                              'ndk_revision': revision, 'license': 'Apache-2.0 WITH LLVM-exception; retained NDK notices apply',
                              'notices': copied}}


def build_spec_projection(spec, target_id, profile_name):
    profile = spec['profiles'][profile_name]
    target = next(value for value in spec['targets'] if value['id'] == target_id)
    return {'ffmpeg': spec['ffmpeg'], 'source_date_epoch': spec['source_date_epoch'],
            'libraries': spec['libraries'], 'configure': spec['configure'], 'encoders': spec['encoders'],
            'filters': spec['filters'], 'profile': profile,
            'dependencies': {name: spec['dependencies'][name] for name in profile['dependencies']},
            'target': {name: value for name, value in target.items() if name != 'validation_status'}}


def verify_sdk_spec(prefix, metadata, current_spec):
    original_path = prefix / 'corresponding-source/linked-spec.json'
    if digest(original_path) != metadata.get('spec_sha256'):
        raise ValueError('Stored SDK specification does not match recorded build hash')
    original = json.loads(original_path.read_text())
    try:
        same = build_spec_projection(original, metadata['target_id'], metadata['profile']) == build_spec_projection(current_spec, metadata['target_id'], metadata['profile'])
    except (KeyError, StopIteration) as error:
        raise ValueError('Unknown SDK target/profile in recorded specification') from error
    if not same:
        raise ValueError('Selected SDK target/profile recipe changed; rebuild the linked SDK')


def verified_runtime_dependencies(prefix, metadata, target):
    """Return SDK-owned runtime files only after checking bytes, target and notices."""
    records = metadata.get('runtime_dependencies', {})
    if target['platform'] == 'android':
        if metadata.get('cxx_runtime') != 'c++_shared' or set(records) != {'libc++_shared.so'}:
            raise ValueError('Android SDK must bundle its single shared NDK libc++ runtime')
    elif records:
        raise ValueError('Unexpected SDK runtime dependency inventory')
    files = {}
    canonical = prefix.resolve()
    for name, record in records.items():
        path = prefix / record['path']
        if not path.resolve().is_relative_to(canonical) or digest(path) != record['sha256']:
            raise ValueError('Bundled C++ runtime escapes SDK or fails integrity verification')
        validate_file(path, target)
        if record.get('ndk_revision') != '28.2.13676358' or not record.get('notices'):
            raise ValueError('Bundled NDK runtime lacks exact revision/license notices')
        for relative, expected in record['notices'].items():
            notice = prefix / relative
            if not notice.resolve().is_relative_to(canonical) or digest(notice) != expected:
                raise ValueError('Bundled NDK runtime license notice integrity mismatch')
        files[name] = path.resolve()
    return files
