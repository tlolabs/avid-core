#!/usr/bin/env python3
"""Execute managed lifecycle tests against the exact archive intended for promotion."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
from artifact import require, validate_runtime, validate_sources
from build import ROOT, SPEC_PATH, digest

TESTS = ['source_built_runtime_satisfies_the_embedded_core_contract',
         'separate_resources_are_required_without_colocated_manifest_fallback',
         'installed_runtime_render_replacement_rollback_and_cleanup',
         'active_native_render_cancellation_releases_runtime_after_worker_join']


def qualify(archive):
    spec=json.loads(SPEC_PATH.read_text())
    # Target identity comes from the package name, then every embedded identity
    # and PE/ELF/Mach-O machine is independently validated below.
    target=next(t['id'] for t in spec['targets'] if archive.name.endswith('-'+t['id']+'.tar.gz'))
    revision=subprocess.check_output(['git','-C',str(ROOT),'rev-parse','HEAD'],text=True).strip()
    files=validate_runtime(archive,spec,target,revision,clean=True)
    source=archive.parent/json.loads(files['SOURCE.json'])['asset']
    require(digest(source)==json.loads(files['SOURCE.json'])['sha256'],'Source/runtime mismatch')
    validate_sources(source,spec,target)
    runtime_hash=digest(archive)
    log=archive.with_name(archive.name+'.installation.log')
    with tempfile.TemporaryDirectory(prefix='avid-installed-') as temporary:
        staged=Path(temporary)/'runtime';staged.mkdir()
        for name,data in files.items():
            path=staged/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(data)
            if name in {'ffmpeg','ffprobe'}:path.chmod(0o755)
        location=str(staged)
        if target.startswith('windows-'):
            location=subprocess.check_output(['cygpath','-w',location],text=True).strip()
        env=dict(os.environ,AVID_RUNTIME_DIRECTORY=location)
        command=['cargo','test','--locked','--test','runtime_contract','--','--ignored','--test-threads=1']
        p=subprocess.run(command,cwd=ROOT,env=env,capture_output=True,timeout=300)
        # Keep an allowlisted result receipt, never raw compiler/panic output.
        output=p.stdout.decode(errors='replace')
        log.write_text(json.dumps({'exit':p.returncode, 'tests':{name:
            'passed' if 'test '+name+' ... ok' in output else 'not_passed'
            for name in TESTS}},indent=2)+'\n')
        require(p.returncode==0,'Installed archive lifecycle failed; see '+str(log))
        output=p.stdout.decode(errors='replace')
        require(all('test '+name+' ... ok' in output for name in TESTS),
                'Required installed lifecycle tests did not execute')
        require(all((staged/name).read_bytes()==data for name,data in files.items()),
                'Installed payload changed during qualification')
    require(digest(archive)==runtime_hash,'Archive changed during qualification')
    report={'schema':1,'status':'passed','target':target,'core_revision':revision,
            'spec_sha256':digest(SPEC_PATH),'runtime_sha256':runtime_hash,'source_sha256':digest(source),
            'binary_sha256':json.loads(files['validation.json'])['binary_sha256'],
            'native_host':json.loads(files['build.json'])['native_host'],
            'tests':TESTS,'command':command,'log':log.name,'log_sha256':digest(log)}
    path=archive.with_name(archive.name+'.qualification.json')
    path.write_text(json.dumps(report,indent=2)+'\n')
    print('Exact archive installation/lifecycle qualification passed:',path)


def verify_receipt(directory, archive, files):
    path=archive.with_name(archive.name+'.qualification.json')
    require(path.is_file(),'Missing installed archive qualification: '+archive.name)
    r=json.loads(path.read_text());b=json.loads(files['build.json']);v=json.loads(files['validation.json'])
    require(r.get('status')=='passed' and r.get('target')==b['target'] and
            r.get('core_revision')==b['core_revision'] and r.get('spec_sha256')==b['spec_sha256'] and
            r.get('runtime_sha256')==digest(archive) and
            r.get('source_sha256')==json.loads(files['SOURCE.json'])['sha256'] and
            r.get('binary_sha256')==v['binary_sha256'] and r.get('tests')==TESTS and
            r.get('native_host')==b.get('native_host') and bool(r.get('native_host')),
            'Installed archive qualification identity mismatch')
    require(r.get('log')==archive.name+'.installation.log' and
            digest(directory/r['log'])==r.get('log_sha256'), 'Installation test log mismatch')
    return r


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('archive',type=Path)
    qualify(p.parse_args().archive.resolve())
