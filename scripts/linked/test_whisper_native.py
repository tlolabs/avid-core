#!/usr/bin/env python3
"""Opt-in real SDK allocation/worker fault test; no model acquisition or helper runtime."""
import argparse
import json
import platform
import shlex
import subprocess
from pathlib import Path

from runtime import verify_inference_patch
from source import ROOT, digest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, required=True)
    parser.add_argument('--model', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True, help='Native test binary outside the source tree')
    parser.add_argument('--cxx', default='clang++')
    args = parser.parse_args()
    prefix = args.prefix.resolve()
    metadata = json.loads((prefix / 'avid-whisper-build.json').read_text())
    machine = {'arm64': 'aarch64', 'aarch64': 'aarch64', 'x86_64': 'x86_64', 'AMD64': 'x86_64'}.get(platform.machine())
    suffix = {'Darwin': 'apple-darwin', 'Linux': 'unknown-linux-gnu'}.get(platform.system())
    if not machine or not suffix or metadata['target'] != machine + '-' + suffix:
        raise ValueError('Fault harness requires a native macOS/Linux SDK matching the executing host')
    verify_inference_patch(prefix, metadata)
    if args.model.stat().st_size != 77704715 or digest(args.model) != '921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f':
        raise ValueError('Fault harness requires the exact explicitly installed tiny.en model')
    libraries = []
    for name in ('libwhisper.a', 'libggml.a', 'libggml-cpu.a', 'libggml-base.a'):
        path = prefix / 'lib' / name
        if not path.resolve().is_relative_to(prefix) or digest(path) != metadata['libraries'][name]:
            raise ValueError('Changed or escaped native inference archive: ' + name)
        libraries.append(str(path))
    output = args.output.resolve()
    if output.is_relative_to(ROOT):
        raise ValueError('Keep generated native test binaries outside the reviewed source tree')
    output.parent.mkdir(parents=True, exist_ok=True)
    command = shlex.split(args.cxx) + ['-std=c++17', '-O1', '-I' + str(ROOT / 'crates/avid-whisper-bridge/native'),
        '-I' + str(prefix / 'include'), str(ROOT / 'scripts/linked/test_whisper_exceptions.cpp'),
        str(ROOT / 'crates/avid-whisper-bridge/native/whisper_bridge.cpp'), *libraries, '-o', str(output)]
    command += ['-framework', 'Accelerate'] if suffix == 'apple-darwin' else ['-pthread', '-lm', '-ldl']
    subprocess.run(command, check=True)
    subprocess.run([str(output), str(args.model.resolve())], check=True, timeout=180)


if __name__ == '__main__':
    main()
