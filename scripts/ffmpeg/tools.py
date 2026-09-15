#!/usr/bin/env python3
"""Install only the manifest-pinned CMake build tool, not media binaries."""
import argparse
import json
from pathlib import Path
import subprocess
import tarfile
import zipfile
from build import ROOT, SPEC_PATH, download

if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--target', required=True)
    p.add_argument('--destination', type=Path, required=True)
    a = p.parse_args()
    spec = json.loads(SPEC_PATH.read_text())
    key = 'macos' if a.target.startswith('macos-') else a.target
    item = spec['build_tools']['cmake_archives'][key]
    cache = ROOT / '.ffmpeg-work/downloads'
    cache.mkdir(parents=True, exist_ok=True)
    archive = download(item, cache)
    a.destination.mkdir(parents=True, exist_ok=False)
    if archive.suffix == '.zip':
        with zipfile.ZipFile(archive) as z:
            for name in z.namelist():
                if name.startswith('/') or '..' in Path(name).parts:
                    raise ValueError('Unsafe tool archive')
            z.extractall(a.destination)
    else:
        with tarfile.open(archive) as t:
            t.extractall(a.destination, filter='data')
    name = 'cmake.exe' if a.target.startswith('windows-') else 'cmake'
    binaries = [p for p in a.destination.rglob(name) if p.is_file() and p.parent.name == 'bin']
    if len(binaries) != 1:
        raise ValueError('Ambiguous CMake archive layout')
    print(binaries[0].parent.resolve())
