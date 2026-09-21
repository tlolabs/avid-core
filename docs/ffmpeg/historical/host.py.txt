#!/usr/bin/env python3
"""Verify/stage canonical runtime payloads before host signing; record signed hashes separately."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
from artifact import require, validate_payload
from build import SPEC_PATH, digest


def verify(directory,target,candidate=False):
    spec=json.loads(SPEC_PATH.read_text())
    if not candidate:
        require(spec['status'] in {'qualified','release-gated'} and not spec['qualification_blockers'],'Core runtime is not qualified')
        require((directory/'acquisition.json').is_file(),'Verified Core acquisition receipt required')
    files={}
    for line in (directory/'SHA256SUMS').read_text().splitlines():
        m=re.fullmatch(r'([0-9a-f]{64})  (.+)',line)
        require(m is not None,'Invalid runtime checksum entry')
        name=m[2]
        require(not name.startswith('/') and '\\' not in name and ':' not in name and '..' not in PurePosixPath(name).parts,
                'Unsafe checksum path')
        path=directory/name
        require(not path.is_symlink() and path.resolve().is_relative_to(directory.resolve()) and name not in files,'Unsafe or duplicate runtime file')
        data=path.read_bytes();require(hashlib.sha256(data).hexdigest()==m[1],'Runtime changed before signing: '+name)
        files[name]=data
    revision=None if candidate else subprocess.check_output(['git','rev-parse','HEAD'],cwd=SPEC_PATH.parents[2],text=True).strip()
    validate_payload(files,spec,target,revision,clean=not candidate)
    if not candidate:
        from release_manifest import verify_manifest
        manifest=verify_manifest(json.loads((directory/'release-manifest.json').read_text()),spec,revision)
        receipt=json.loads((directory/'acquisition.json').read_text())
        require(receipt.get('core_revision')==revision and receipt.get('target')==target and
                receipt.get('repository')==spec['release_repository'] and bool(receipt.get('assets')) and receipt['assets'].get('manifest.json',{}).get('sha256')==digest(directory/'release-manifest.json') and
                all(receipt['assets'].get(manifest['targets'][target][k]['asset'],{}).get('sha256')==manifest['targets'][target][k]['sha256'] for k in ['runtime','sources']),'Mismatched acquisition receipt')
    return files


def stage(directory,target,destination,candidate=False):
    files=verify(directory,target,candidate)
    destination.mkdir(parents=True,exist_ok=True)
    # Permit adding a runtime to a host staging directory, but never replace any runtime files.
    names=list(files)+['SHA256SUMS']+(['acquisition.json','release-manifest.json'] if (directory/'acquisition.json').exists() else [])
    require(all(not (destination/name).exists() for name in names),'Destination already contains runtime files')
    for name in names:
        path=destination/name;path.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(directory/name,path)


def record_signed(directory,target):
    suffix='.exe' if target.startswith('windows-') else ''
    original=json.loads((directory/'validation.json').read_text())['binary_sha256']
    report={'schema':1,'target':target,'original_binary_sha256':original,
            'signed_binary_sha256':{n+suffix:digest(directory/(n+suffix)) for n in ['ffmpeg','ffprobe']},
            'scope':'Host signing changes; original release identity and checksums remain intact. Host verifies platform signatures.'}
    (directory/'signed-payload.json').write_text(json.dumps(report,indent=2)+'\n')


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('mode',choices=['verify','stage','record-signed']);p.add_argument('target');p.add_argument('directory',type=Path);p.add_argument('--destination',type=Path);p.add_argument('--candidate',action='store_true',help='Local qualification only; never accepts an unvalidated binary')
    a=p.parse_args()
    if a.mode=='verify':verify(a.directory,a.target,a.candidate)
    elif a.mode=='stage':
        require(a.destination is not None,'Staging destination required');stage(a.directory,a.target,a.destination,a.candidate)
    else:record_signed(a.directory,a.target)
