"""Record recipe-6 executions without qualifying or changing the runtime."""
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

root = Path.cwd()
out = root / 'diagnostics'
out.mkdir(exist_ok=True)
runtime = root / 'dist/avid-ffmpeg-9.0.1-r6-windows-x86_64'
ff = runtime / 'ffmpeg.exe'
probe = runtime / 'ffprobe.exe'
records = []
expected = {'ffmpeg.exe': 'e559803304c98e70ca1b649ccbbc8c6351895d7706aadf69d9742258cd40546c',
            'ffprobe.exe': '6dd4e1adb72b73979d301b424958078f827fdc6a9e4dd5e207e18efa7a6cf6a5'}
hashes = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in [ff, probe]}
(out/'identity.json').write_text(json.dumps({'original': expected, 'reconstructed': hashes,
    'same_binaries': hashes == expected, 'platform': platform.platform(), 'machine': platform.machine()}, indent=2))

def run(name, exe, args):
    command = list(map(str, [exe, *args]))
    try:
        p = subprocess.run(command, stdin=subprocess.DEVNULL, capture_output=True, timeout=120)
        record = {'name':name, 'command':command, 'returncode':p.returncode,
                  'windows_status':hex(p.returncode & 0xffffffff)}
        stdout, stderr = p.stdout, p.stderr
    except subprocess.TimeoutExpired as e:
        record = {'name':name, 'command':command, 'timeout':120}
        stdout, stderr = e.stdout or b'', e.stderr or b''
    (out/(name+'.stdout')).write_bytes(stdout)
    (out/(name+'.stderr')).write_bytes(stderr)
    records.append(record)
    (out/'commands.json').write_text(json.dumps(records, indent=2))
    print(json.dumps(record), flush=True)
    return record

run('ffmpeg-version',ff,['-version'])
run('ffprobe-version',probe,['-version'])
for category in ['codecs','formats','encoders','decoders','filters','buildconf','cpuflags']:
    if category != 'cpuflags': run(category,ff,['-hide_banner','-'+category])
art=out/'artwork ü red.png'; wav=out/'track 523.wav'
run('generated-audio',ff,['-y','-f','lavfi','-i','sine=frequency=523:duration=1','-c:a','pcm_s16le',wav])
run('generated-image',ff,['-y','-f','lavfi','-i','testsrc2=s=192x128','-frames:v','1',art])
run('generated-video',ff,['-y','-f','lavfi','-i','testsrc2=s=192x128:d=1','-c:v','libx264',out/'simple.mp4'])
run('decode-video',ff,['-i',out/'simple.mp4','-f','null','-'])
run('probe-video',probe,['-v','error','-show_streams','-show_format','-of','json',out/'simple.mp4'])
graph='[0:v]hflip,vflip,split=2[bgsrc][fgsrc];[bgsrc]scale=90:160:force_original_aspect_ratio=increase,crop=90:160,gblur=sigma=40[bg];[fgsrc]scale=90:90:force_original_aspect_ratio=decrease[fg];[bg][fg]overlay=(W-w)/2:(H-h)/2,format=yuv420p[video]'
args=['-nostdin','-hide_banner','-loglevel','warning','-y','-loop','1','-framerate','60','-protocol_whitelist','file,pipe','-i',art,'-protocol_whitelist','file,pipe','-i',wav,'-filter_complex',graph,'-map','[video]','-map','1:a:0','-c:v','libx264','-tune','stillimage','-pix_fmt','yuv420p','-c:a','aac','-b:a','128k','-shortest','-movflags','+faststart','-f','mp4','-progress','pipe:1','-nostats',out/'portrait.mp4']
crashes=[]
r=run('portrait-original',ff,args)
if r.get('windows_status')=='0xc0000005': crashes.append(r)
# Invocation-only controls retain the exact binary, dimensions and filter path.
run('portrait-cpuflags-zero',ff,['-cpuflags','0',*args])
run('portrait-one-filter-thread',ff,['-filter_complex_threads','1',*args])
for name, vf in [
    ('blur90','gblur=sigma=40'),
    ('blur96','gblur=sigma=40'),
    ('flips','hflip,vflip'),
    ('scale-crop','hflip,vflip,scale=90:160:force_original_aspect_ratio=increase,crop=90:160'),
    ('scale-crop-blur','hflip,vflip,scale=90:160:force_original_aspect_ratio=increase,crop=90:160,gblur=sigma=40'),
]:
    size='96x160' if name=='blur96' else '90x160' if name=='blur90' else '192x128'
    cmd=['-v','error','-f','lavfi','-i','testsrc2=s='+size+':d=0.1','-vf',vf,'-frames:v','1','-f','null','-']
    r=run(name,ff,cmd)
    if r.get('windows_status')=='0xc0000005':
        crashes.append(r)
        run(name+'-cpuflags-zero',ff,['-cpuflags','0',*cmd])
# Capture debugger state for the smallest observed reproducer and the original.
lldb=shutil.which('lldb')
for r in (crashes[:1]+crashes[-1:] if len(crashes)>1 else crashes):
    if lldb:
        debugger=['--batch','-o','run','-o','thread backtrace all','-o','register read','-o','disassemble --pc --count 20',
                  '-k','thread backtrace all','-k','register read','-k','disassemble --pc --count 20','--',*r['command']]
        run('lldb-'+r['name'],lldb,debugger)
    else:
        (out/'debugger-unavailable.txt').write_text('No LLDB on PATH; native debugger still required.\n')
# Record CPU and disassembly/symbol maps, including unstripped original link products.
run('cpu','powershell.exe',['-NoProfile','-Command','Get-CimInstance Win32_Processor | Format-List *'])
for exe in [ff, probe]: run(exe.stem+'-imports','llvm-objdump',['-p',exe])
work = Path(os.environ['AVID_NATIVE_WORK'])
for exe in work.glob('ffmpeg/*/ffmpeg_g.exe'):
    shutil.copy2(exe,out/exe.name)
    run('ffmpeg-symbols','llvm-nm',['-n',exe])
    run('ffmpeg-disassembly','llvm-objdump',['-d',exe])
# Always preserve the tested pair. These files are diagnostics, never release candidates.
for exe in [ff,probe]: shutil.copy2(exe,out/exe.name)
