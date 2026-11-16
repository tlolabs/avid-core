#!/usr/bin/env python3
"""Build the screen-free iOS integration app against a target-specific linked Core static library."""
import argparse
import json
from pathlib import Path
import plistlib
import subprocess

from binary import validate_file
from source import ROOT, SPEC_PATH, run


def build(args):
    spec = json.loads(SPEC_PATH.read_text())
    target = next(t for t in spec['targets'] if t['id'] == args.target)
    if target['platform'] not in ('ios', 'ios-simulator'):
        raise ValueError('Select an iOS device or simulator target')
    validate_file(args.core_library, target)
    app = args.output.resolve()
    app.mkdir(parents=True, exist_ok=False)
    sdk = target['sdk']
    arch = 'arm64' if target['arch'] == 'arm64' else 'x86_64'
    triple = arch + '-apple-ios' + target['minimum'] + ('-simulator' if target['platform'] == 'ios-simulator' else '')
    command = ['xcrun', '--sdk', sdk, 'swiftc', '-parse-as-library', '-target', triple,
        '-sdk', run(['xcrun', '--sdk', sdk, '--show-sdk-path']), '-module-cache-path', str(app.parent / (app.stem + '-ModuleCache')), '-I', str(ROOT / 'include'),
        str(ROOT / 'examples/apple-linked/AVIDCore.swift'), str(ROOT / 'examples/apple-linked/ios/App.swift'),
        str(args.core_library.resolve()), '-o', str(app / 'AVIDLinkedHost'), '-lz', '-liconv', '-lc++']
    for framework in ('Foundation', 'UIKit', 'AVFoundation', 'AudioToolbox', 'VideoToolbox', 'CoreFoundation', 'CoreMedia', 'CoreVideo', 'Security', 'SystemConfiguration'):
        command += ['-framework', framework]
    subprocess.run(command, check=True)
    info = {'CFBundleIdentifier': 'org.avidcore.LinkedHost', 'CFBundleExecutable': 'AVIDLinkedHost',
            'CFBundleName': 'AVID Linked Host', 'CFBundlePackageType': 'APPL', 'CFBundleVersion': '1',
            'CFBundleShortVersionString': '1.0', 'MinimumOSVersion': target['minimum'],
            'UIDeviceFamily': [1, 2], 'UILaunchScreen': {}, 'LSRequiresIPhoneOS': True,
            'CFBundleSupportedPlatforms': ['iPhoneSimulator' if target['platform'] == 'ios-simulator' else 'iPhoneOS']}
    (app / 'Info.plist').write_bytes(plistlib.dumps(info))
    if target['platform'] == 'ios-simulator':
        subprocess.run(['codesign', '--force', '--sign', '-', str(app)], check=True)
    else:
        print('Device app is unsigned. Sign using an authorized iOS development identity/profile before install.')
    print('Built development integration host (not a distribution artifact):', app)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target', required=True)
    parser.add_argument('--core-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    build(parser.parse_args())
