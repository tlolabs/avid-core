#!/usr/bin/env python3
"""Download one complete successful native matrix by immutable Actions artifact digest."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import stat
import subprocess
import zipfile
from artifact import require
from build import SPEC_PATH


def gh(*args):
    return subprocess.check_output(['gh',*map(str,args)])


def retrieve(run_id, output):
    require(str(run_id).isdigit(),'Numeric qualification run ID required')
    spec=json.loads(SPEC_PATH.read_text());repo=spec['release_repository']
    revision=os.environ['GITHUB_SHA']
    run=json.loads(gh('api',f'repos/{repo}/actions/runs/{run_id}'))
    require(run.get('head_sha')==revision and run.get('path')=='.github/workflows/ffmpeg.yml' and
            run.get('repository',{}).get('full_name')==repo, 'Qualification run source identity mismatch')
    require(run.get('conclusion')=='success' or str(run_id)==os.environ.get('GITHUB_RUN_ID'),
            'Only a successful prior qualification run can be promoted')
    jobs=json.loads(gh('api',f'repos/{repo}/actions/runs/{run_id}/jobs?per_page=100'))['jobs']
    for target in spec['targets']:
        matches=[j for j in jobs if j['name']==f'build ({target["id"]})']
        require(len(matches)==1 and matches[0].get('conclusion')=='success', 'Native qualification job did not pass: '+target['id'])
    artifacts=json.loads(gh('api',f'repos/{repo}/actions/runs/{run_id}/artifacts?per_page=100'))['artifacts']
    output.mkdir(parents=True,exist_ok=False)
    receipts=[]
    for target in spec['targets']:
        matches=[a for a in artifacts if a['name']=='runtime-'+target['id']]
        require(len(matches)==1 and not matches[0]['expired'],'Missing unique retained runtime artifact')
        a=matches[0]
        data=gh('api',f'repos/{repo}/actions/artifacts/{a["id"]}/zip')
        require(a.get('digest')=='sha256:'+hashlib.sha256(data).hexdigest(),'Actions archive SHA-256 mismatch')
        with zipfile.ZipFile(io.BytesIO(data)) as z:
            for member in z.infolist():
                name=member.filename
                require('/' not in name and '\\' not in name and ':' not in name and name not in {'.','..'} and
                        not stat.S_ISLNK(member.external_attr>>16) and not (output/name).exists(),
                        'Unsafe or duplicate qualification artifact member')
                (output/name).write_bytes(z.read(member))
        receipts.append({'id':a['id'],'name':a['name'],'sha256':a['digest']})
    (output/'qualification-run.json').write_text(json.dumps({'run_id':str(run_id),'core_revision':revision,
        'url':run['html_url'],'artifacts':receipts},indent=2)+'\n')


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('run_id');p.add_argument('output',type=Path)
    a=p.parse_args();retrieve(a.run_id,a.output)
