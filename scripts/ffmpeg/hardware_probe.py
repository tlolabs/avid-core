#!/usr/bin/env python3
"""Record actual encoder outcomes; lack of hardware is evidence, never a passing device gate."""
import argparse
import json
from pathlib import Path
import platform
import subprocess
import tempfile
from build import SPEC_PATH, digest
from validate import names, capture


def probe(directory,target):
    spec=json.loads(SPEC_PATH.read_text());t=next(t for t in spec['targets'] if t['id']==target)
    suffix='.exe' if target.startswith('windows-') else ''
    ff=directory/('ffmpeg'+suffix)
    available=names(capture(ff,'-hide_banner','-encoders'))
    result={'schema':1,'target':target,'platform':platform.platform(),'binary_sha256':{n+suffix:digest(directory/(n+suffix)) for n in ['ffmpeg','ffprobe']},'encoders':{}}
    with tempfile.TemporaryDirectory(prefix='avid-hardware-') as d:
        for encoder in t['required_encoders']:
            if not encoder.startswith(('h264_','hevc_')):continue
            if encoder not in available:
                result['encoders'][encoder]={'status':'failed','reason':'not advertised'};continue
            args=[str(ff),'-nostdin','-hide_banner','-loglevel','verbose','-y','-f','lavfi','-i','color=c=red:s=160x90:r=24:d=0.5',
                  '-c:v',encoder,'-pix_fmt','yuv420p']
            if encoder.endswith('_videotoolbox'):args+=['-allow_sw','0']
            args += [str(Path(d)/(encoder+'.mp4'))]
            try:
                p=subprocess.run(args,stdin=subprocess.DEVNULL,capture_output=True,timeout=45)
                result['encoders'][encoder]={'status':'passed' if p.returncode==0 else 'failed','exit_code':p.returncode,
                                             'command':args[:-1]+['<temporary-output>'],'stderr':p.stderr.decode(errors='replace')}
            except subprocess.TimeoutExpired as error:
                result['encoders'][encoder]={'status':'failed','reason':'timeout','stderr':(error.stderr or b'').decode(errors='replace')}
    result['status']='passed' if result['encoders'] and all(v['status']=='passed' for v in result['encoders'].values()) else 'failed'
    result['scope']='Short real encoder invocation; not minimum-OS, full Core workflow, or driver-version-range qualification.'
    return result


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--directory',type=Path,required=True);p.add_argument('--target',required=True);p.add_argument('--report',type=Path,required=True);a=p.parse_args()
    report=probe(a.directory.resolve(),a.target);a.report.parent.mkdir(parents=True,exist_ok=True);a.report.write_text(json.dumps(report,indent=2)+'\n');print('Hardware observation:',report['status'])
