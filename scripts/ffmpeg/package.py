#!/usr/bin/env python3
"""Package a validated Core runtime candidate. Publication remains separately gated."""
import argparse
import gzip
import json
from pathlib import Path
import tarfile
from artifact import validate_payload, validate_runtime, validate_sources
from build import SPEC_PATH, artifact_name, digest


def package(directory, output):
    spec = json.loads(SPEC_PATH.read_text())
    build = json.loads((directory / 'build.json').read_text())
    target = build['target']
    files = {p.relative_to(directory).as_posix(): p.read_bytes()
             for p in directory.rglob('*') if p.is_file() and p.name != 'SHA256SUMS'}
    validate_payload(files, spec, target, clean=True)
    sources = directory.parent / (artifact_name(spec, target) + '-sources.tar.gz')
    validate_sources(sources, spec, target)
    checksums = ''.join(f'{digest(directory / name)}  {name}\n' for name in sorted(files))
    (directory / 'SHA256SUMS').write_text(checksums)
    output.mkdir(parents=True, exist_ok=True)
    path = output / (artifact_name(spec, target) + '.tar.gz')

    def normalize(info):
        info.uid = info.gid = 0
        info.uname = info.gname = ''
        info.mtime = spec['source_date_epoch']
        return info

    with path.open('wb') as raw, gzip.GzipFile(filename='', mode='wb', fileobj=raw,
                                               mtime=spec['source_date_epoch']) as compressed, \
            tarfile.open(fileobj=compressed, mode='w') as archive:
        archive.add(directory, arcname=directory.name, filter=normalize)
    validate_runtime(path, spec, target, core_revision=build['core_revision'], clean=True)
    (path.with_name(path.name + '.sha256')).write_text(f'{digest(path)}  {path.name}\n')
    print('Validated Core runtime candidate:', path)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    package(args.directory.resolve(), args.output.resolve())
