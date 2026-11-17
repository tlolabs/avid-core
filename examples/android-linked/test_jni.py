#!/usr/bin/env python3
"""Run the real JNI lifecycle regression on a desktop JVM, without Android claims.

Requires an explicit Core shared library built for this host and a JDK 17+.
Does not acquire dependencies or invoke media executables. Build tools may use
PATH; the JVM executes with an empty PATH. No release or notarization is performed.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--java-home', type=Path, required=True)
    parser.add_argument('--core-library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True, help='New work directory; existing directories are rejected')
    parser.add_argument('--cc', default='cc')
    args = parser.parse_args()
    if sys.platform not in ('darwin', 'linux'):
        parser.error('This desktop transport check currently supports macOS/Linux; use the NDK for Android')
    java = args.java_home.resolve()
    core = args.core_library.resolve(strict=True)
    suffix = '.dylib' if sys.platform == 'darwin' else '.so'
    if core.suffix != suffix:
        parser.error('Pass the native Core shared library, not a static archive or cross-target artifact')
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    classes = root / 'classes'; classes.mkdir()
    work = root / 'work'; work.mkdir()
    example = Path(__file__).resolve().parent
    include = example.parents[1] / 'include'
    jni_platform = 'darwin' if sys.platform == 'darwin' else 'linux'
    library = root / ('libavid_core_jni' + suffix)
    commands = [
        [args.cc, '-std=c11', '-Wall', '-Wextra', '-Werror', '-fPIC',
         '-dynamiclib' if sys.platform == 'darwin' else '-shared',
         '-I' + str(java / 'include'), '-I' + str(java / 'include' / jni_platform),
         '-I' + str(include), str(example / 'linked_jni.c'), str(core),
         '-Wl,-rpath,' + str(core.parent), '-o', str(library)],
        [str(java / 'bin/javac'), '--release', '17', '-Xlint:all', '-Werror', '-d', str(classes),
         str(example / 'src/org/avidcore/LinkedCore.java'), str(example / 'tests/org/avidcore/JniLifecycleTest.java')],
        [str(java / 'bin/java'), '--enable-native-access=ALL-UNNAMED', '-Djava.awt.headless=true',
         '-Djava.library.path=' + str(root), '-cp', str(classes),
         'org.avidcore.JniLifecycleTest', str(work)],
    ]
    (root / 'commands.json').write_text(json.dumps(commands, indent=2))
    for index, command in enumerate(commands):
        env = dict(os.environ, PATH=str(root / 'empty-path')) if index == 2 else None
        with (root / f'step-{index}.log').open('wb') as log:
            result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT)
        if result.returncode:
            print((root / f'step-{index}.log').read_text(errors='replace'), file=sys.stderr)
            raise SystemExit(result.returncode)
    result = {'kind': 'desktop-jni-lifecycle-test', 'status': 'passed', 'platform': sys.platform,
              'android_compilation': 'not_run', 'android_runtime': 'not_run',
              'media_executables_on_path': False, 'work': str(work)}
    (root / 'result.json').write_text(json.dumps(result, indent=2))
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
