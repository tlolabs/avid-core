#!/usr/bin/env python3
"""Deliberate native Simple-timeline memory regression; no media executables.

Supply a built Core shared library with software H.264 support. Four fresh worker
processes measure distinct low/high-resolution artwork paths at one/sixteen clips. The check
subtracts low-resolution graph overhead and permits four full-resolution RGB
frames of extra growth; it catches retaining a decoded image for every consumed
clip without imposing a universal RSS ceiling. macOS/Linux use getrusage peak
RSS; Windows uses GetProcessMemoryInfo peak working set. This is an opt-in native
measurement, not cross-platform qualification or an application memory guarantee.
"""
import argparse
import ctypes
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import wave
import zlib


def peak_bytes():
    if os.name != 'nt':
        import resource
        peak = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
        return peak if sys.platform == 'darwin' else peak * 1024
    from ctypes import wintypes
    class Counters(ctypes.Structure):
        _fields_ = [('cb', wintypes.DWORD), ('PageFaultCount', wintypes.DWORD)] + [
            (name, ctypes.c_size_t) for name in ('PeakWorkingSetSize', 'WorkingSetSize',
            'QuotaPeakPagedPoolUsage', 'QuotaPagedPoolUsage', 'QuotaPeakNonPagedPoolUsage',
            'QuotaNonPagedPoolUsage', 'PagefileUsage', 'PeakPagefileUsage')]
    kernel, psapi = ctypes.WinDLL('kernel32'), ctypes.WinDLL('psapi')
    kernel.GetCurrentProcess.restype = wintypes.HANDLE
    psapi.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.POINTER(Counters), wintypes.DWORD]
    counters = Counters()
    counters.cb = ctypes.sizeof(counters)
    if not psapi.GetProcessMemoryInfo(kernel.GetCurrentProcess(), ctypes.byref(counters), counters.cb):
        raise ctypes.WinError()
    return counters.PeakWorkingSetSize


def worker(library, request):
    # Add only the supplied package directory, never PATH FFmpeg discovery.
    dll_directory = os.add_dll_directory(str(library.parent)) if os.name == 'nt' else None
    core = ctypes.CDLL(str(library))
    core.avid_core_abi_version.restype = ctypes.c_uint32
    if core.avid_core_abi_version() != 1:
        raise RuntimeError('This test requires Core ABI 1')
    for name in ('avid_core_create', 'avid_core_cancel_create'):
        getattr(core, name).restype = ctypes.c_uint64
    core.avid_core_release.argtypes = [ctypes.c_uint64]
    core.avid_core_execute_json.argtypes = [ctypes.c_uint64, ctypes.c_uint64, ctypes.c_void_p,
                                           ctypes.c_size_t, ctypes.c_void_p, ctypes.c_void_p]
    core.avid_core_execute_json.restype = ctypes.c_uint64
    core.avid_core_buffer_len.argtypes = [ctypes.c_uint64]
    core.avid_core_buffer_len.restype = ctypes.c_size_t
    core.avid_core_buffer_copy.argtypes = [ctypes.c_uint64, ctypes.c_void_p, ctypes.c_size_t]
    core.avid_core_buffer_copy.restype = ctypes.c_size_t
    renderer, cancel, result = core.avid_core_create(), core.avid_core_cancel_create(), 0
    try:
        data = request.read_bytes()
        result = core.avid_core_execute_json(renderer, cancel, data, len(data), None, None)
        size = core.avid_core_buffer_len(result)
        if not result or not size:
            raise RuntimeError('Core returned no response')
        buffer = ctypes.create_string_buffer(size)
        if core.avid_core_buffer_copy(result, buffer, size) != size:
            raise RuntimeError('Core response copy failed')
        response = json.loads(buffer.raw)
        if not response.get('ok'):
            raise RuntimeError(response)
    finally:
        for handle in (result, cancel, renderer):
            if handle:
                core.avid_core_release(handle)
        if dll_directory:
            dll_directory.close()
    print(json.dumps({'peak_rss_bytes': peak_bytes(), 'response': response}))


def artwork(path, dimension):
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    compressor, compressed = zlib.compressobj(), bytearray()
    row = b'\0' + b'\xff\0\0' * dimension
    for _ in range(dimension):
        compressed.extend(compressor.compress(row))
    compressed.extend(compressor.flush())
    header = struct.pack('>IIBBBBB', dimension, dimension, 8, 2, 0, 0, 0)
    path.write_bytes(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', header) + chunk(b'IDAT', compressed) + chunk(b'IEND', b''))


def exercise(library):
    with tempfile.TemporaryDirectory(prefix='avid-memory-') as folder:
        root = Path(folder)
        audio = root / 'silence.wav'
        with wave.open(str(audio), 'wb') as output:
            output.setparams((1, 2, 16000, 0, 'NONE', 'not compressed'))
            output.writeframes(b'\0' * 16000)
        results = {}
        for dimension in (64, 2048):
            image = root / f'art-{dimension}.png'
            artwork(image, dimension)
            for count in (1, 16):
                # Distinct source paths prevent preparation-cache deduplication
                # from hiding retained contexts between sequential compositions.
                images = []
                for index in range(count):
                    unique = root / f'art-{dimension}-{index}.png'
                    unique.write_bytes(image.read_bytes())
                    images.append(unique)
                request = root / f'request-{dimension}-{count}.json'
                request.write_text(json.dumps({
                    'schema_version': 1, 'operation': 'render', 'mode': 'simple',
                    'clips': [{'id': str(index), 'image': str(images[index]), 'audio': str(audio),
                               'duration_seconds': 0.5} for index in range(count)],
                    'output': str(root / f'output-{dimension}-{count}.mp4'),
                    'settings': {'width': 160, 'height': 90, 'fps': 24, 'encoding': 'software'}}))
                child = subprocess.run([sys.executable, str(Path(__file__).resolve()),
                    '--worker', str(library), str(request)], capture_output=True, check=True,
                    env=dict(os.environ, PATH=str(root / 'empty-path')))
                results[f'{dimension}/{count}'] = json.loads(child.stdout)
        growth = {size: results[f'{size}/16']['peak_rss_bytes'] - results[f'{size}/1']['peak_rss_bytes']
                  for size in (64, 2048)}
        excess = growth[2048] - growth[64]
        allowance = 4 * 2048 * 2048 * 3
        report = {'measurements': results, 'growth_bytes': growth,
                  'resolution_related_growth_bytes': excess, 'allowance_bytes': allowance,
                  'measurement': 'peak_working_set' if os.name == 'nt' else 'getrusage_peak_rss',
                  'media_executables_on_path': False, 'passed': excess <= allowance}
        print(json.dumps(report, indent=2))
        if not report['passed']:
            raise SystemExit('Simple timeline retains excessive full-resolution data per consumed clip')


if __name__ == '__main__':
    if len(sys.argv) == 4 and sys.argv[1] == '--worker':
        worker(Path(sys.argv[2]), Path(sys.argv[3]))
    else:
        parser = argparse.ArgumentParser(description=__doc__)
        parser.add_argument('--library', type=Path, required=True)
        exercise(parser.parse_args().library.resolve(strict=True))
