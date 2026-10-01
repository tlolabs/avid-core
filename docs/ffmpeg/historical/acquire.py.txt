#!/usr/bin/env python3
"""Acquire this Core checkout's qualified immutable runtime. Inputs: target and new destination."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
from artifact import require, validate_runtime, validate_sources
from build import ROOT, SPEC_PATH, artifact_name, digest


def gh(*args):
    return subprocess.check_output(['gh',*map(str,args)])


def acquire(target,destination):
    spec=json.loads(SPEC_PATH.read_text())
    require(spec['status'] in {'qualified','release-gated'} and not spec['qualification_blockers'],'Runtime qualification is incomplete; preserve the working host runtime')
    require(target in {t['id'] for t in spec['targets']},'Unsupported runtime target')
    destination=destination.absolute()
    require(not destination.exists(),'Destination already exists; acquisition never replaces a working bundle or cache')
    revision=subprocess.check_output(['git','-C',str(ROOT),'rev-parse','HEAD'],text=True).strip()
    require(not subprocess.check_output(['git','-C',str(ROOT),'status','--porcelain','--','runtime/ffmpeg','scripts/ffmpeg']).strip(),
            'Selected Core runtime mapping or acquisition code is modified')
    repo=spec['release_repository'];tag=f'ffmpeg-{spec["source"]["version"]}-r{spec["recipe"]}'
    release=json.loads(gh('api',f'repos/{repo}/releases/tags/{tag}'))
    require(not release['draft'] and not release['prerelease'] and release.get('immutable') is True,'A published immutable Core runtime release is required')
    # Resolve annotated/lightweight tags through the authenticated repository API.
    commit=json.loads(gh('api',f'repos/{repo}/commits/{tag}'))['sha']
    require(commit==revision,'Release tag does not identify the selected Core revision')
    assets={a['name']:a for a in release['assets']}
    name=artifact_name(spec,target)
    names=['manifest.json',name+'.tar.gz',name+'-sources.tar.gz']
    destination.parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.avid-acquire-',dir=destination.parent) as temporary:
        work=Path(temporary);receipts={}
        for filename in names:
            require(filename in assets,'Missing Core release asset: '+filename)
            asset=assets[filename]
            require(asset.get('digest','').startswith('sha256:'),'Missing trusted GitHub release asset SHA-256')
            path=work/filename
            subprocess.run(['gh','release','download',tag,'--repo',repo,'--pattern',filename,'--dir',str(work)],check=True)
            require('sha256:'+digest(path)==asset['digest'],'Release artifact SHA-256 mismatch')
            verification=json.loads(gh('attestation','verify',path,'--repo',repo,
                '--signer-workflow',repo+'/.github/workflows/ffmpeg.yml','--source-digest',revision,
                '--signer-digest',revision,'--deny-self-hosted-runners','--format','json'))
            require(bool(verification),'No verified build attestation')
            receipts[filename]={'asset_id':asset['id'],'sha256':digest(path),'attestations':verification}
        from release_manifest import verify_manifest
        manifest=verify_manifest(json.loads((work/'manifest.json').read_text()),spec,revision)
        entry=manifest['targets'][target]
        for kind in ['runtime','sources']:
            require(digest(work/entry[kind]['asset'])==entry[kind]['sha256'],'Downloaded artifact differs from qualified manifest')
        files=validate_runtime(work/names[1],spec,target,revision,clean=True)
        source=work/names[2]
        require(json.loads(files['SOURCE.json'])['sha256']==digest(source),'Source/runtime pair mismatch')
        validate_sources(source,spec,target)
        staged=work/'runtime';staged.mkdir()
        # Write already-validated regular entries only. No archive extraction can follow links.
        for filename,data in files.items():
            path=staged/filename;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(data)
            if filename in {'ffmpeg','ffprobe'}:path.chmod(0o755)
        (staged/'acquisition.json').write_text(json.dumps({'schema':1,'repository':repo,'release_id':release['id'],
            'tag':tag,'core_revision':revision,'target':target,'assets':receipts},indent=2)+'\n')
        shutil.copy2(work/'manifest.json',staged/'release-manifest.json')
        # Preserve actual corresponding source with the acquired runtime for host redistribution.
        shutil.copy2(source,staged/source.name)
        require(not destination.exists(),'Destination appeared during verification')
        staged.rename(destination)
    print(destination)


if __name__ == '__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('target');p.add_argument('destination',type=Path)
    a=p.parse_args();acquire(a.target,a.destination)
