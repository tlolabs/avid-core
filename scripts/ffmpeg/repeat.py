#!/usr/bin/env python3
"""Rebuild from pristine sources at the same prefix, then compare both executables."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys
from build import ROOT, SPEC_PATH, artifact_name, digest


def repeat(target, work, first):
    spec = json.loads(SPEC_PATH.read_text())
    # Only delete a build tree created and explicitly marked by this recipe.
    marker = work/'.avid-build-root'
    if not marker.is_file() or marker.read_text() != target or work.is_symlink():
        raise ValueError('Refusing to clean an unowned build directory')
    old = json.loads((first/'build.json').read_text())
    if old['spec_sha256'] != digest(SPEC_PATH):
        raise ValueError('Specification changed between builds')
    shutil.rmtree(work)
    output = ROOT/'dist/repeat'
    subprocess.run([sys.executable, str(ROOT/'scripts/ffmpeg/build.py'), '--target',target,
                    '--work',str(work),'--output',str(output)],check=True)
    second = output/artifact_name(spec,target)
    subprocess.run([sys.executable,str(ROOT/'scripts/ffmpeg/validate.py'),'--target',target,
                    '--directory',str(second),'--report',str(second/'validation.json')],check=True)
    new = json.loads((second/'build.json').read_text())
    for key in ['spec_sha256','core_revision','build_scripts_sha256','tools','build_options','configure']:
        if old[key] != new[key]:
            raise ValueError('Repeat build inputs changed: '+key)
    suffix = '.exe' if target.startswith('windows-') else ''
    hashes = {name+suffix: {'first':digest(first/(name+suffix)), 'second':digest(second/(name+suffix))} for name in ['ffmpeg','ffprobe']}
    passed = all(v['first']==v['second'] for v in hashes.values())
    (first/'repeat-build.json').write_text(json.dumps({'status':'passed' if passed else 'failed',
        'target':target,'spec_sha256':digest(SPEC_PATH),'core_revision':old['core_revision'],
        'scope':'Two clean source builds on the same runner and absolute prefix; not cross-environment reproducibility',
        'binary_sha256':hashes},indent=2)+'\n')
    if not passed:
        raise ValueError('Repeat-build executables differ')


if __name__ == '__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--target',required=True)
    p.add_argument('--work',type=Path,required=True)
    p.add_argument('--first',type=Path,required=True)
    a=p.parse_args();repeat(a.target,a.work.resolve(),a.first.resolve())
