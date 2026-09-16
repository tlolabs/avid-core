"""Controlled object-level experiments after exact recipe-6 crash reproduction."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

work=Path('/tmp/avid-ffmpeg-ci')
ff=next((work/'ffmpeg').glob('ffmpeg-*'))
out=Path.cwd()/'experiments'; out.mkdir(exist_ok=True)
base=(ff/'ffbuild/config.mak').read_text()
flags=next(line.split('=',1)[1] for line in base.splitlines() if line.startswith('CFLAGS='))
results=[]
env=dict(os.environ)
env['PATH']=str(work/'prefix/bin')+os.pathsep+env['PATH']

def command(name,args,cwd=None):
    args=list(map(str,args))
    p=subprocess.run(args,cwd=cwd,env=env,capture_output=True,timeout=300)
    (out/(name+'.stdout')).write_bytes(p.stdout)
    (out/(name+'.stderr')).write_bytes(p.stderr)
    result={'name':name,'command':args,'cwd':str(cwd),'returncode':p.returncode}
    results.append(result)
    (out/'commands.json').write_text(json.dumps(results,indent=2))
    print(json.dumps(result),flush=True)
    return p

def native_args(exe,args):
    native=os.environ['AVID_NATIVE_PYTHON']
    # MSYS Python masks Windows statuses; delegate execution to native Python.
    to_win=lambda path: subprocess.check_output(['cygpath','-w',str(path)],text=True).strip()
    return [native,'-c','import subprocess,sys,json; p=subprocess.run(sys.argv[1:]); print(json.dumps({"returncode":p.returncode,"windows_status":hex(p.returncode&0xffffffff)})); sys.exit(1 if p.returncode else 0)',to_win(exe),*args]

probe=['-v','error','-f','lavfi','-i','testsrc2=s=90x160:d=0.1','-vf','gblur=sigma=40','-frames:v','1','-f','null','-']
for name,extra in [('stock',''),('no-vectorization','-fno-vectorize -fno-slp-vectorize'),('O1','-O1'),('no-builtin-lrintf','-fno-builtin-lrintf'),('no-peephole','-mllvm -disable-peephole')]:
    (ff/'libavfilter/vf_gblur.o').unlink(missing_ok=True)
    p=command(name+'-compile',['make','V=1','libavfilter/vf_gblur.o','CFLAGS='+flags+' '+extra],ff)
    if p.returncode: continue
    command(name+'-object',['llvm-objdump','-d',ff/'libavfilter/vf_gblur.o'])
    p=command(name+'-link',['make','-j','8','ffmpeg.exe'],ff)
    if p.returncode: continue
    exe=out/(name+'.exe'); shutil.copy2(ff/'ffmpeg.exe',exe)
    command(name+'-blur90',native_args(exe,probe))
    command(name+'-blur96',native_args(exe,[x.replace('90x160','96x160') for x in probe]))
    if name=='stock':
        actual=hashlib.sha256(exe.read_bytes()).hexdigest()
        (out/'stock-identity.json').write_text(json.dumps({'actual':actual,'expected':'e559803304c98e70ca1b649ccbbc8c6351895d7706aadf69d9742258cd40546c'}))
# Retain inputs needed for further controlled relinking, not release promotion.
for name in ['ffbuild/config.mak','ffbuild/config.log','config.h','config_components.h']:
    dest=out/name; dest.parent.mkdir(parents=True,exist_ok=True); shutil.copy2(ff/name,dest)
