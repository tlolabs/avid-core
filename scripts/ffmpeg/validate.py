#!/usr/bin/env python3
"""Fail closed on identity, capability, functional or dependency failures."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import platform
import re
import subprocess
import tempfile
import wave
import struct
from build import ROOT, SPEC_PATH, digest


def capture(executable, *args):
    p = subprocess.run([str(executable), *map(str, args)], stdin=subprocess.DEVNULL,
                       capture_output=True, timeout=120)
    if p.returncode:
        raise RuntimeError(f'{executable.name} {args}: {p.stderr.decode(errors="replace")}')
    return p.stdout.decode(errors='replace')


def names(text):
    result = set()
    for line in text.splitlines():
        fields = line.split()
        if len(fields) >= 2 and re.fullmatch(r'[A-Z.]{1,6}', fields[0]):
            result.update(fields[1].split(','))
        elif len(fields) == 1 and re.fullmatch(r'[a-z][a-z0-9_]*', fields[0]):
            result.add(fields[0])
    return result


def require(condition, message):
    if not condition:
        raise ValueError(message)


def smoke(ff, probe, target):
    report = {}
    with tempfile.TemporaryDirectory(prefix='avid-media-') as temporary:
        d = Path(temporary)
        wav = d / 'source ü.wav'
        with wave.open(str(wav), 'wb') as output:
            output.setparams((1, 2, 44100, 44100, 'NONE', 'not compressed'))
            output.writeframes(b''.join(struct.pack('<h', round(8000 * math.sin(2*math.pi*440*i/44100))) for i in range(44100)))
        def encode(*args):
            return capture(ff, '-nostdin', '-v', 'error', '-y', *args)
        def inspect(path):
            return json.loads(capture(probe, '-v', 'error', '-show_streams', '-show_format', '-show_chapters', '-of', 'json', path))
        def pcm(path):
            dest = d / 'transcript.wav'
            encode('-i', path, '-vn', '-ar', '16000', '-ac', '1', '-c:a', 'pcm_s16le', dest)
            with wave.open(str(dest), 'rb') as w:
                require(w.getframerate() == 16000 and w.getnchannels() == 1 and w.getsampwidth() == 2, 'Transcript PCM format changed')
                require(15000 <= w.getnframes() <= 19000, 'Decoded audio duration changed')
        for codec, ext in [('pcm_s16le','wav'),('pcm_s24le','wav'),('pcm_s32le','wav'),
                           ('pcm_f32le','wav'),('pcm_f64le','wav'),('pcm_s16be','aiff'),
                           ('pcm_s24be','aiff'),('pcm_s32be','aiff'),('pcm_f32be','aiff'),
                           ('pcm_f64be','aiff'),('flac','flac'),('libmp3lame','mp3'),('aac','m4a')]:
            path = d / (codec + '.' + ext)
            encode('-i', wav, '-c:a', codec, path)
            pcm(path)
            report[codec] = inspect(path)['streams'][0]['codec_name']
        for path in sorted((ROOT / 'tests/fixtures/ffmpeg').glob('*')):
            if path.suffix in ('.ogg', '.opus', '.wma', '.m4a'):
                pcm(path)
        images = []
        for codec, ext in [('png','png'),('mjpeg','jpg'),('bmp','bmp'),('tiff','tiff')]:
            path = d / ('cover.' + ext)
            encode('-f', 'lavfi', '-i', 'color=c=red:s=96x64', '-frames:v', '1', '-c:v', codec, path)
            images.append(path)
        images += sorted((ROOT / 'tests/fixtures/ffmpeg').glob('*.webp'))
        for art in images:
            value = capture(probe, '-v', 'error', '-select_streams', 'v:0', '-show_entries', 'stream=width,height', '-of', 'csv=p=0:s=x', art).strip()
            require(value == '96x64', f'Artwork dimensions changed: {art}: {value}')
            encode('-i', art, '-vf', 'scale=64:48', '-frames:v', '1', d / 'preview.png')
        duration = capture(probe, '-v', 'error', '-select_streams', 'a:0', '-show_entries', 'stream=duration:format=duration', '-of', 'default=noprint_wrappers=1:nokey=1', wav)
        require(any(abs(float(v)-1) < .01 for v in duration.split() if v != 'N/A'), 'Duration probe contract changed')
        meta = d / 'chapters.txt'
        meta.write_text(';FFMETADATA1\nalbum=Podcast\ntitle=Episode ü\ncomment=Summary\n[CHAPTER]\nTIMEBASE=1/1000\nSTART=0\nEND=1000\ntitle=Intro\nurl=https://example.invalid/chapter\n', encoding='utf-8')
        for codec, ext in [('libmp3lame','mp3'),('aac','m4a')] + ([('aac_at','m4a')] if target.startswith('macos-') else []):
            dest = d / (codec + '-chapters.' + ext)
            extra = ['-id3v2_version','3'] if ext == 'mp3' else ['-movflags','+faststart']
            encode('-i', wav, '-i', wav, '-i', images[0], '-f', 'ffmetadata', '-i', meta,
                   '-filter_complex', '[0:a]aformat=sample_rates=48000:channel_layouts=stereo,asetpts=PTS-STARTPTS[a0];[1:a]aformat=sample_rates=48000:channel_layouts=stereo,asetpts=PTS-STARTPTS[a1];[a0][a1]concat=n=2:v=0:a=1[outa]',
                   '-map','[outa]','-map','2:v:0','-c:v','copy','-disposition:v','attached_pic',
                   '-map_metadata','3','-map_chapters','3','-c:a',codec,'-b:a','192k',*extra,dest)
            info = inspect(dest)
            require(info['format']['tags'].get('title') == 'Episode ü', 'Metadata title was not preserved')
            require(info['chapters'][0]['tags']['title'] == 'Intro', 'Chapter title was not preserved')
            require(any(s.get('disposition',{}).get('attached_pic') == 1 for s in info['streams']), 'Attached artwork missing')
            require(1.9 < float(info['format']['duration']) < 2.2, 'Audio concat duration changed')
            report[codec+'-metadata'] = 'passed'
        for codec in ['libx264','libx265']:
            dest = d / (codec + '.mp4')
            extra = ['-tune','stillimage'] if codec == 'libx264' else ['-tag:v','hvc1']
            progress = encode('-loop','1','-framerate','24','-i',images[0],'-i',wav,
                '-vf','hflip,vflip,scale=160:90,fps=24,format=yuv420p','-map','0:v','-map','1:a:0',
                '-c:v',codec,*extra,'-c:a','aac','-b:a','128k','-t','1','-shortest',
                '-movflags','+faststart','-progress','pipe:1','-nostats',dest)
            require('progress=end' in progress and 'out_time_us=' in progress, 'Progress protocol changed')
            info = inspect(dest)
            video = next(s for s in info['streams'] if s['codec_type'] == 'video')
            require(video['codec_name'] == ('h264' if codec == 'libx264' else 'hevc'), 'Video codec changed')
            require(video['width'] == 160 and video['height'] == 90, 'Scale result changed')
            require(video['r_frame_rate'] == '24/1', 'Frame rate changed')
            if codec == 'libx265':
                require(video['codec_tag_string'] == 'hvc1', 'HEVC MP4 tag changed')
            encode('-i',dest,'-f','null','-')
            report[codec+'-video'] = 'passed'
    return report


def validate(args):
    spec = json.loads(SPEC_PATH.read_text())
    target = next(t for t in spec['targets'] if t['id'] == args.target)
    directory = args.directory.resolve()
    suffix = '.exe' if target['os'] == 'windows' else ''
    ff, probe = (directory / ('ffmpeg'+suffix), directory / ('ffprobe'+suffix))
    report = {'target': args.target, 'baseline': args.baseline, 'binary_sha256': {}, 'capabilities': {}}
    if not args.baseline:
        require(json.loads((directory/'spec.json').read_text()) == spec, 'Package specification differs from this Core revision')
        build = json.loads((directory/'build.json').read_text())
        require(build['spec_sha256'] == digest(SPEC_PATH) and build['target'] == args.target and
                build['source_revision'] == spec['source']['revision'], 'Build provenance mismatch')
        for parser in spec['required']['parsers']:
            require(parser in build['parsers'], f'Missing configured parser: {parser}')
    for tool in (ff, probe):
        report['binary_sha256'][tool.name] = digest(tool)
        text = capture(tool, '-version')
        report[tool.stem + '_version'] = text.splitlines()[0]
        if not args.baseline:
            require(text.split()[2] == spec['source']['version'], 'Unexpected FFmpeg source version')
    for category, required in spec['required'].items():
        if category == 'parsers':
            continue  # FFmpeg CLI has no -parsers; verify generated config + decoding tests.
        text = capture(ff, '-hide_banner', '-' + category)
        available = names(text)
        missing = set(required) - available
        if category == 'encoders':
            missing |= set(target['required_encoders']) - available
        require(not missing, f'Missing {category}: {sorted(missing)}')
        report['capabilities'][category] = sorted(available)
    configuration = capture(ff, '-hide_banner', '-buildconf')
    report['buildconf'] = configuration
    if not args.baseline:
        require('--enable-nonfree' not in configuration and '--enable-version3' not in configuration, 'Unapproved license flags')
        require('--enable-gpl' in configuration and '--disable-autodetect' in configuration, 'Build policy missing')
        if target['os'] == 'macos':
            linkage = '\n'.join(subprocess.check_output(['otool','-L',str(t)],text=True) for t in (ff,probe))
            for line in linkage.splitlines():
                if line.startswith('\t'):
                    lib = line.strip().split(' (')[0]
                    require(lib.startswith(('/usr/lib/','/System/Library/')), f'Non-system runtime dependency: {lib}')
        elif target['os'] == 'linux':
            linkage = '\n'.join(subprocess.check_output(['ldd',str(t)],text=True) for t in (ff,probe))
            for line in linkage.splitlines():
                name = line.strip().split(' ')[0]
                require(name.startswith(('linux-vdso','/lib','libc.so','libm.so','libmvec.so','libpthread.so','libdl.so','librt.so','libstdc++.so','libgcc_s.so')),
                        f'Unexpected runtime dependency: {line}')
                require('not found' not in line, f'Missing runtime dependency: {line}')
        else:
            linkage = '\n'.join(subprocess.check_output(['llvm-objdump','-p',str(t)],text=True) for t in (ff,probe))
            for name in re.findall(r'DLL Name:\s*(\S+)', linkage):
                require(name.lower() in {'kernel32.dll','msvcrt.dll','ucrtbase.dll','advapi32.dll','shell32.dll','ole32.dll','user32.dll','ws2_32.dll','bcrypt.dll','secur32.dll','ncrypt.dll','oleaut32.dll'} or name.lower().startswith('api-ms-win-'), f'Unbundled Windows dependency: {name}')
        report['linkage'] = linkage
    report['smoke'] = smoke(ff, probe, args.target)
    args.report.parent.mkdir(parents=True,exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2)+'\n')
    print('Validation passed:', args.report)


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--directory', type=Path, required=True)
    p.add_argument('--target', required=True)
    p.add_argument('--report', type=Path, required=True)
    p.add_argument('--baseline', action='store_true', help='Behavioral comparison only; never valid for publication')
    validate(p.parse_args())
