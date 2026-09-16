#!/usr/bin/env python3
"""Observe optional macOS VideoToolbox output; never qualify Windows/Linux GPUs."""
import argparse
import json
from pathlib import Path
import platform
import subprocess
import tempfile
from build import SPEC_PATH, digest
from validate import names, capture, require


def probe(directory, target):
    spec = json.loads(SPEC_PATH.read_text())
    t = next(t for t in spec['targets'] if t['id'] == target)
    result = {'schema': 2, 'target': target, 'required': False, 'encoders': {}}
    if t['os'] != 'macos':
        result.update(status='not_required', reason='Windows and Linux hardware encoding is outside current project scope.')
        return result
    result.update(platform=platform.platform(), scope='Optional VideoToolbox encode, stream inspection and full decode. No byte/size parity requirement; not minimum-OS qualification.')
    try:
        ff, fp = directory/'ffmpeg', directory/'ffprobe'
        result['binary_sha256'] = {name: digest(directory/name) for name in ['ffmpeg', 'ffprobe']}
        available = names(capture(ff, '-hide_banner', '-encoders'))
        with tempfile.TemporaryDirectory(prefix='avid-videotoolbox-') as d:
            for encoder in t.get('optional_encoders', []):
                if encoder not in {'h264_videotoolbox', 'hevc_videotoolbox'}:
                    raise ValueError('Only VideoToolbox is an optional hardware encoder')
                if encoder not in available:
                    result['encoders'][encoder] = {'status': 'unavailable', 'reason': 'not advertised'}
                    continue
                dest = Path(d)/(encoder+'.mp4')
                args = [str(ff), '-nostdin', '-v', 'error', '-y', '-f', 'lavfi', '-i',
                        'color=c=red:s=160x90:r=24:d=0.5', '-c:v', encoder, '-pix_fmt', 'yuv420p', '-allow_sw', '0']
                if encoder.startswith('hevc'):
                    args += ['-tag:v', 'hvc1']
                args += [str(dest)]
                try:
                    subprocess.run(args, stdin=subprocess.DEVNULL, capture_output=True, timeout=45, check=True)
                    info = json.loads(capture(fp, '-v', 'error', '-show_streams', '-of', 'json', dest))
                    video = next(s for s in info['streams'] if s['codec_type'] == 'video')
                    require(video['codec_name'] == ('h264' if encoder.startswith('h264') else 'hevc'), 'Wrong codec')
                    require((video['width'], video['height']) == (160, 90), 'Wrong dimensions')
                    require(video['pix_fmt'] == 'yuv420p' and video['r_frame_rate'] == '24/1', 'Wrong pixel format or frame rate')
                    require(0.4 <= float(video['duration']) <= 0.6, 'Wrong duration')
                    if encoder.startswith('hevc'):
                        require(video['codec_tag_string'] == 'hvc1', 'Wrong HEVC MP4 tag')
                    capture(ff, '-nostdin', '-v', 'error', '-xerror', '-i', dest, '-f', 'null', '-')
                    result['encoders'][encoder] = {'status': 'passed', 'codec': video['codec_name'], 'decoded': True}
                except (OSError, subprocess.SubprocessError, RuntimeError, ValueError, KeyError, StopIteration) as error:
                    result['encoders'][encoder] = {'status': 'failed', 'reason': str(error),
                        'stderr': (getattr(error, 'stderr', None) or b'').decode(errors='replace')}
        states = [e['status'] for e in result['encoders'].values()]
        result['status'] = 'passed' if states and all(s == 'passed' for s in states) else 'failed' if 'failed' in states else 'unavailable'
    except (OSError, subprocess.SubprocessError, RuntimeError, ValueError) as error:
        result.update(status='unavailable', reason=str(error))
    return result


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--directory', type=Path, required=True)
    p.add_argument('--target', required=True)
    p.add_argument('--report', type=Path, required=True)
    a = p.parse_args()
    report = probe(a.directory.resolve(), a.target)
    a.report.parent.mkdir(parents=True, exist_ok=True)
    a.report.write_text(json.dumps(report, indent=2)+'\n')
    print('Optional hardware observation:', report['status'])
