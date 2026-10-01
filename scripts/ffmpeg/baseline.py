#!/usr/bin/env python3
"""Inventory historical distributed binaries in a separate temporary directory."""
import argparse
import json
from pathlib import Path
import tarfile
import tempfile
import zipfile
from build import ROOT, download, digest
from validate import capture, names


def inventory(target, cache):
    items = json.loads((ROOT/'runtime/ffmpeg/baseline.json').read_text())['targets']
    item = items[target]
    cache.mkdir(parents=True, exist_ok=True)
    archive = download(item, cache)
    with tempfile.TemporaryDirectory(prefix='avid-baseline-') as temporary:
        root = Path(temporary)
        if archive.suffix == '.zip':
            with zipfile.ZipFile(archive) as z:
                for name in z.namelist():
                    if name.startswith('/') or '..' in Path(name).parts:
                        raise ValueError('Unsafe baseline archive')
                z.extractall(root)
        else:
            with tarfile.open(archive) as tar:
                tar.extractall(root, filter='data')
        result = {'target': target, 'archive': item, 'binary_sha256': {}, 'inventory': {}}
        suffix = '.exe' if target.startswith('windows-') else ''
        for name in ['ffmpeg', 'ffprobe']:
            paths = list(root.rglob(name+suffix))
            if len(paths) != 1:
                raise ValueError('Ambiguous baseline pair')
            binary = paths[0]
            result['binary_sha256'][name+suffix] = digest(binary)
            result['inventory'][name+'-version'] = capture(binary, '-version')
            if name == 'ffmpeg':
                for category in ['buildconf', 'encoders', 'hwaccels']:
                    result['inventory'][category] = capture(binary, '-hide_banner', '-'+category)
                encoders = names(result['inventory']['encoders'])
                result['preserve_encoders'] = sorted(n for n in encoders if n.startswith(('h264_', 'hevc_')) and n.endswith(('_nvenc','_amf','_qsv','_vaapi','_videotoolbox')))
        return result


if __name__ == '__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--target', required=True)
    p.add_argument('--cache', type=Path, required=True)
    p.add_argument('--report', type=Path, required=True)
    a=p.parse_args()
    result=inventory(a.target,a.cache)
    a.report.parent.mkdir(parents=True,exist_ok=True)
    a.report.write_text(json.dumps(result,indent=2)+'\n')
    print('Historical inventory:',result['preserve_encoders'])
