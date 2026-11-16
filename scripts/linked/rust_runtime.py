"""Identify and preserve notices for the Rust runtime embedded by staticlib/cdylib.

Rust std, core, alloc and compiler_builtins are sysroot components, not Cargo
workspace dependencies; cargo vendor cannot supply their redistribution notices.
"""
import json
from pathlib import Path
import shutil
import subprocess

from source import ROOT, digest


def inspect_rust_runtime(target, sysroot=None, identity=None):
    if identity is None:
        identity = subprocess.check_output(['rustc', '-Vv'], cwd=ROOT, text=True).strip()
    if sysroot is None:
        sysroot = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], cwd=ROOT, text=True).strip())
    sysroot = Path(sysroot).resolve()
    library_directory = sysroot / 'lib/rustlib' / target / 'lib'
    libraries = sorted(library_directory.glob('*.rlib'))
    for component in ('std', 'core', 'alloc', 'compiler_builtins'):
        if not any(path.name.startswith('lib' + component + '-') for path in libraries):
            raise ValueError('Selected Rust target runtime lacks ' + component)
    copyright_directory = sysroot / 'share/doc/rust'
    notices = [copyright_directory / 'COPYRIGHT-library.html', copyright_directory / 'COPYRIGHT.html']
    license_directory = copyright_directory / 'licenses'
    for name in ('MIT.txt', 'Apache-2.0.txt', 'LLVM-exception.txt'):
        if not (license_directory / name).is_file():
            raise ValueError('Selected Rust toolchain lacks required runtime license text: ' + name)
    notices += sorted(license_directory.glob('*.txt'))
    if any(not path.is_file() or not path.stat().st_size for path in notices):
        raise ValueError('Selected Rust toolchain lacks its complete runtime copyright notices')
    manifest = sysroot / 'lib/rustlib' / ('manifest-rust-std-' + target)
    if not manifest.is_file():
        raise ValueError('Selected Rust target runtime installation manifest is missing')
    return {'schema': 1, 'kind': 'rust-sysroot-runtime', 'target': target, 'rustc': identity,
            'inventory_scope': 'Installed target runtime libraries; final binary embeds a subset. Official notices are retained inclusively.',
            'available_target_runtime_libraries': {path.name: digest(path) for path in libraries},
            'target_component_manifest_sha256': digest(manifest),
            'notices': {path.relative_to(sysroot).as_posix(): digest(path) for path in notices}}


def copy_rust_runtime(record, destination, sysroot=None, identity=None):
    if sysroot is None:
        sysroot = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], cwd=ROOT, text=True).strip())
    sysroot = Path(sysroot).resolve()
    if inspect_rust_runtime(record['target'], sysroot, identity) != record:
        raise ValueError('Rust target runtime or notices differ from the recorded Core build')
    destination.mkdir(parents=True, exist_ok=False)
    for relative, expected in record['notices'].items():
        path = sysroot / relative
        output = destination / Path(relative).relative_to('share/doc/rust')
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, output)
        if digest(output) != expected:
            raise ValueError('Rust runtime notice changed during copy')
    manifest = sysroot / 'lib/rustlib' / ('manifest-rust-std-' + record['target'])
    shutil.copy2(manifest, destination / 'rust-std-install-manifest.txt')
    (destination / 'runtime-inventory.json').write_text(json.dumps(record, indent=2) + '\n')
