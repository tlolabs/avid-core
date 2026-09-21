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


def validate_qualification(qualification, target_id, binary_sha256, gates=None):
    """Optional hardware never gates promotion; exact software/OS evidence does."""
    for gate in (gates if gates is not None else ['software_encoding','minimum_os','toolchain','host_packaging']):
        entry=qualification.get('targets',{}).get(target_id,{}).get(gate,{})
        if entry.get('status')!='passed' or not entry.get('evidence'):
            raise ValueError('Unperformed qualification gate: '+target_id+' '+gate)
        for evidence in entry['evidence']:
            path=(ROOT/evidence['path']).resolve()
            if not path.is_relative_to(ROOT/'docs/ffmpeg') or digest(path)!=evidence['sha256']:
                raise ValueError('Invalid qualification evidence')
            report=json.loads(path.read_text())
            if (report.get('status')!='passed' or report.get('target')!=target_id or
                report.get('gate')!=gate or report.get('binary_sha256')!=binary_sha256):
                raise ValueError('Qualification evidence does not cover this exact executable pair')


def promote(directory, host_packaging=None):
    spec = json.loads(SPEC_PATH.read_text())
    if spec['status'] not in {'qualified','release-gated'} or spec['qualification_blockers']:
        raise ValueError('Publication blocked: '+ '; '.join(spec['qualification_blockers']))
    revision = os.environ.get('GITHUB_SHA') or subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
    policy=dict(spec.get('qualification_policy',{}))
    if host_packaging is not None:
        if host_packaging not in {'required','downstream'}:
            raise ValueError('Unknown application packaging qualification scope')
        policy['host_packaging']=host_packaging
    if policy.get('host_packaging')=='downstream':
        policy['scope']='Qualified Core runtime; application signing, packaging, launch and updates remain downstream gates'
    targets={}
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
        from qualify_archive import verify_receipt
        receipt=verify_receipt(directory, directory/(name+'.tar.gz'), files)
        qualification=json.loads((ROOT/'runtime/ffmpeg/qualification.json').read_text())
        if policy.get('host_packaging')!='downstream':
            validate_qualification(qualification, target['id'], json.loads(files['validation.json'])['binary_sha256'],
                gates=['host_packaging'] if spec['status']=='release-gated' else None)
        sources=directory/(name+'-sources.tar.gz')
        if json.loads(files['SOURCE.json'])['sha256'] != digest(sources):
            raise ValueError('Corresponding-source package mismatch')
        validate_sources(sources,spec,target['id'])
        targets[target['id']]={'status':'passed','qualification_os':target['qualification_os'],
            'native_host':receipt['native_host'],'binary_sha256':receipt['binary_sha256'],
            'runtime':{'asset':name+'.tar.gz','sha256':receipt['runtime_sha256']},
            'sources':{'asset':sources.name,'sha256':receipt['source_sha256']},
            'qualification':{'asset':name+'.tar.gz.qualification.json','sha256':digest(directory/(name+'.tar.gz.qualification.json'))},
            'provenance':'build.json, source-provenance.json, validation.json and repeat-build.json inside the runtime archive'}
    from release_manifest import MANIFEST, verify_manifest
    manifest={'schema':1,'status':'qualified','tag':f'ffmpeg-{spec["source"]["version"]}-r{spec["recipe"]}',
        'core_revision':revision,'core_version':__import__('tomllib').loads((ROOT/'Cargo.toml').read_text())['package']['version'],
        'spec_sha256':digest(SPEC_PATH),'recipe':spec['recipe'],'source':spec['source'],
        'qualification_policy':policy,'targets':targets}
    verify_manifest(manifest,spec,revision)
    (directory/MANIFEST).write_text(json.dumps(manifest,indent=2)+'\n')
    (directory/'SHA256SUMS').write_text(''.join(f'{digest(p)}  {p.name}\n' for p in sorted(directory.iterdir()) if p.is_file() and p.name!='SHA256SUMS'))
    print('Complete qualified runtime matrix verified')


if __name__ == '__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('mode',choices=['package','promote'])
    p.add_argument('directory',type=Path)
    p.add_argument('--output',type=Path,default=Path('dist/packages'))
    p.add_argument('--host-packaging',choices=['required','downstream'],help='Explicit release scope decision; defaults to the checked-in prerequisite')
    a=p.parse_args()
    package(a.directory,a.output) if a.mode=='package' else promote(a.directory,a.host_packaging)
