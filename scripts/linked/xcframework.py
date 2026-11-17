#!/usr/bin/env python3
"""Combine validated Apple static Core candidates into an XCFramework without mixing SDKs."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

from binary import deployment_target, validate_file
from source import digest


def create(packages, output):
    if output.exists():
        raise ValueError('Fresh XCFramework output path required')
    slices = {}
    identity = None
    for directory in packages:
        metadata = json.loads((directory / 'linked-artifact.json').read_text())
        target = metadata['target']
        if metadata.get('kind') != 'avid-linked-core-artifact' or target['platform'] not in ('macos', 'ios', 'ios-simulator'):
            raise ValueError('Expected linked Apple candidate')
        current_identity = (metadata['abi_version'], metadata['core_source']['sha256'], metadata['ffmpeg']['profile'], metadata['ffmpeg']['spec_sha256'], tuple(metadata['core_build'].get('features', [])),
                            (metadata.get('inference') or {}).get('metadata', {}).get('source_revision'))
        if identity is not None and identity != current_identity:
            raise ValueError('XCFramework slices must share ABI/source/profile identities')
        identity = current_identity
        for name, expected in metadata['files'].items():
            if digest(directory / name) != expected:
                raise ValueError('Slice file checksum differs: ' + name)
        library = directory / 'lib/libavid_core_ffi.a'
        validate_file(library, deployment_target(target, metadata['minimum_runtime']['combined']))
        key = target['platform']
        if any(t['arch'] == target['arch'] for _, t, _ in slices.get(key, [])):
            raise ValueError('Duplicate platform/architecture slice')
        slices.setdefault(key, []).append((library, target, directory / 'include'))
    if set(slices) != {'macos', 'ios', 'ios-simulator'}:
        raise ValueError('Supply macOS, iOS device, and iOS simulator slices separately')
    if {t['arch'] for _, t, _ in slices['macos']} != {'arm64', 'x86_64'}:
        raise ValueError('XCFramework requires both supported macOS architectures')
    if {t['arch'] for _, t, _ in slices['ios-simulator']} != {'arm64', 'x86_64'}:
        raise ValueError('XCFramework requires both supported simulator architectures')
    command = ['xcodebuild', '-create-xcframework']
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='avid-linked-xc-') as temporary:
        for platform, entries in sorted(slices.items()):
            directory = Path(temporary) / platform
            directory.mkdir()
            library = directory / 'libavid_core_ffi.a'
            headers = directory / 'Headers'
            shutil.copytree(entries[0][2], headers)
            for _, _, other in entries[1:]:
                for header in headers.iterdir():
                    if digest(header) != digest(other / header.name):
                        raise ValueError('XCFramework slice header mismatch')
            if len(entries) == 1:
                shutil.copy2(entries[0][0], library)
            else:
                subprocess.run(['lipo', '-create', *[str(path) for path, _, _ in entries], '-output', str(library)], check=True)
            command += ['-library', str(library), '-headers', str(headers)]
        subprocess.run(command + ['-output', str(output.resolve())], check=True)
    # Distribution signing/notarization belong to the containing app. No signing,
    # notarization, or publication is performed by this library packager.
    (output / 'AVIDSourceIdentity.json').write_text(json.dumps({'schema': 1, 'abi_version': identity[0],
        'core_source_sha256': identity[1], 'profile': identity[2], 'spec_sha256': identity[3],
        'features': list(identity[4]), 'inference_revision': identity[5],
        'qualification': 'unqualified; consult per-target native evidence'}, indent=2) + '\n')
    print('Created linked XCFramework candidate:', output)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--slice', type=Path, action='append', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    create([path.resolve() for path in args.slice], args.output.resolve())
