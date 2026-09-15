#!/usr/bin/env python3
"""Package a validated candidate; promotion requires the entire qualified matrix."""
import argparse
import gzip
import json
from pathlib import Path
import tarfile
from build import ROOT, SPEC_PATH, digest, artifact_name
from artifact import validate_payload, validate_runtime, validate_sources
import os
import subprocess


def package(directory, output):
    spec = json.loads(SPEC_PATH.read_text())
    build = json.loads((directory / 'build.json').read_text())
    report = json.loads((directory / 'validation.json').read_text())
    if report.get('baseline') is not False or report['target'] != build['target'] or not report.get('smoke'):
        raise ValueError('A non-baseline validation report is required')
    if build['spec_sha256'] != digest(SPEC_PATH):
        raise ValueError('Build specification changed after compilation')
    for name, expected in report['binary_sha256'].items():
        if digest(directory/name) != expected:
            raise ValueError('Binary changed after validation')
    if not (directory/'core-tests-passed.txt').is_file():
        raise ValueError('Core and managed-runtime integration tests must pass before packaging')
    validate_payload({p.relative_to(directory).as_posix():p.read_bytes() for p in directory.rglob('*') if p.is_file()}, spec, build['target'])
    checksums = ''.join(f'{digest(p)}  {p.relative_to(directory).as_posix()}\n' for p in sorted(directory.rglob('*')) if p.is_file() and p.name != 'SHA256SUMS')
    (directory/'SHA256SUMS').write_text(checksums)
    output.mkdir(parents=True,exist_ok=True)
    path = output / (artifact_name(spec, build['target'])+'.tar.gz')
    def normalized(i):
        i.uid=i.gid=0
        i.uname=i.gname=''
        i.mtime=spec['source_date_epoch']
        return i
    with path.open('wb') as raw, gzip.GzipFile(filename='',mode='wb',fileobj=raw,mtime=spec['source_date_epoch']) as gz, tarfile.open(fileobj=gz,mode='w') as t:
        t.add(directory,arcname=directory.name,filter=normalized)
    (path.with_name(path.name+'.sha256')).write_text(f'{digest(path)}  {path.name}\n')
    print(path)


def promote(directory):
    spec = json.loads(SPEC_PATH.read_text())
    if spec['status'] != 'qualified' or spec['qualification_blockers']:
        raise ValueError('Publication blocked: '+ '; '.join(spec['qualification_blockers']))
    revision = os.environ.get('GITHUB_SHA') or subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
    for target in spec['targets']:
        name = artifact_name(spec, target['id'])
        for suffix in ['.tar.gz','-sources.tar.gz']:
            path = directory/(name+suffix)
            if not path.is_file():
                raise ValueError(f'Incomplete matrix: {path.name}')
            checksum = path.with_name(path.name+'.sha256')
            if not checksum.is_file() or checksum.read_text().split()[0] != digest(path):
                raise ValueError(f'Missing or mismatched checksum: {path.name}')
        files=validate_runtime(directory/(name+'.tar.gz'),spec,target['id'],revision,clean=True)
        qualification=json.loads((ROOT/'runtime/ffmpeg/qualification.json').read_text())
        for gate in ['hardware_parity','minimum_os','toolchain','host_packaging']:
            entry=qualification.get('targets',{}).get(target['id'],{}).get(gate,{})
            if entry.get('status')!='passed' or not entry.get('evidence'):
                raise ValueError('Unperformed qualification gate: '+target['id']+' '+gate)
            for evidence in entry['evidence']:
                path=(ROOT/evidence['path']).resolve()
                if not path.is_relative_to(ROOT/'docs/ffmpeg') or digest(path)!=evidence['sha256']:
                    raise ValueError('Invalid qualification evidence')
                report=json.loads(path.read_text())
                if (report.get('status')!='passed' or report.get('target')!=target['id'] or
                    report.get('gate')!=gate or report.get('binary_sha256')!=json.loads(files['validation.json'])['binary_sha256']):
                    raise ValueError('Qualification evidence does not cover this exact executable pair')
        sources=directory/(name+'-sources.tar.gz')
        if json.loads(files['SOURCE.json'])['sha256'] != digest(sources):
            raise ValueError('Corresponding-source package mismatch')
        validate_sources(sources,spec,target['id'])
    print('Complete qualified runtime matrix verified')


if __name__ == '__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('mode',choices=['package','promote'])
    p.add_argument('directory',type=Path)
    p.add_argument('--output',type=Path,default=Path('dist/packages'))
    a=p.parse_args()
    package(a.directory,a.output) if a.mode=='package' else promote(a.directory)
