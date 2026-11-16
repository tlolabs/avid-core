#!/usr/bin/env python3
"""Exercise compiled C/Swift/JNI-compatible JSON hosts with no media tools on PATH.

Hosts are test executables calling Core, never production media helper processes.
Optional independent ffprobe/ffmpeg validation uses explicitly supplied development
verifiers, not the production execution path. No remote inputs or downloads.
"""
import argparse
import json
import math
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import wave
import zlib


def sources(root, seconds):
    def chunk(kind, body):
        return struct.pack('>I', len(body)) + kind + body + struct.pack('>I', zlib.crc32(kind + body))
    image = root / 'cover ü.png'
    rows = b''.join(b'\0' + b'\xff\0\0' * 48 + b'\0\0\xff' * 48 for _ in range(64))
    image.write_bytes(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 96, 64, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b''))
    audio = root / f'audio-{seconds}.wav'
    block = b''.join(struct.pack('<h', int(12000 * math.sin(i * 2 * math.pi * 440 / 44100))) for i in range(44100))
    with wave.open(str(audio), 'wb') as output:
        output.setparams((1, 2, 44100, 0, 'NONE', 'not compressed'))
        for _ in range(seconds):
            output.writeframesraw(block)
    return image, audio


def exercise(host, root, ffprobe=None, ffmpeg=None):
    image, audio = sources(root, 2)
    output = root / 'export ü.mp4'
    request = root / 'request.json'
    request.write_text(json.dumps({'schema_version': 1, 'operation': 'render', 'image': str(image), 'audio': str(audio), 'output': str(output), 'mode': 'simple', 'settings': {'width': 160, 'height': 90, 'fps': 24, 'encoding': 'automatic'}}))
    env = dict(os.environ, PATH=str(root / 'empty-path'))
    completed = subprocess.run([str(host), str(request)], env=env, capture_output=True, check=True)
    result = json.loads(completed.stdout)
    assert result['ok'], result
    events = [json.loads(line) for line in completed.stderr.splitlines() if line.startswith(b'{')]
    assert any(event.get('event') == 'progress' for event in events)
    assert events[-1] == {'event': 'stage', 'stage': 'complete'}
    before = output.read_bytes()
    cancelled = subprocess.run([str(host), '--cancel-on-progress', str(request)], env=env, capture_output=True)
    assert cancelled.returncode == 10, cancelled.stderr.decode(errors='replace')
    result = json.loads(cancelled.stdout)
    assert result['error']['code'] == 'cancelled', result
    assert output.read_bytes() == before
    assert not list(root.glob('.avid-*'))
    if ffprobe:
        metadata = json.loads(subprocess.check_output([str(ffprobe), '-v', 'error', '-show_streams', '-show_format', '-of', 'json', str(output)]))
        video = next(stream for stream in metadata['streams'] if stream['codec_type'] == 'video')
        audio_info = next(stream for stream in metadata['streams'] if stream['codec_type'] == 'audio')
        assert (video['codec_name'], video['width'], video['height'], video['sample_aspect_ratio']) == ('h264', 160, 90, '1:1')
        assert (audio_info['sample_rate'], audio_info['channels']) == ('44100', 1)
        assert abs(float(metadata['format']['duration']) - 2.0) < 0.1
    if ffmpeg:
        subprocess.run([str(ffmpeg), '-v', 'error', '-i', str(output), '-f', 'null', '-'], check=True)
    return {'host': str(host), 'export': 'passed', 'progress': 'passed', 'cancellation_preservation': 'passed', 'media_executables_on_path': False, 'independent_probe': bool(ffprobe), 'independent_decode': bool(ffmpeg)}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', type=Path, required=True, action='append')
    parser.add_argument('--ffprobe', type=Path)
    parser.add_argument('--ffmpeg', type=Path)
    arguments = parser.parse_args()
    results = []
    for binary in arguments.host:
        with tempfile.TemporaryDirectory(prefix='avid-linked-host-') as folder:
            results.append(exercise(binary.resolve(), Path(folder), arguments.ffprobe, arguments.ffmpeg))
    print(json.dumps(results, indent=2))
