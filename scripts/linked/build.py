#!/usr/bin/env python3
"""Build pinned FFmpeg libraries for one target/profile. No publisher or runtime download path."""
import argparse
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess

from runtime import android_runtime, cxx_runtime
from source import ROOT, SPEC_PATH, acquire, authenticate_ffmpeg, digest, extract, run


def execute(command, cwd, env):
    print('+', ' '.join(map(str, command)), flush=True)
    subprocess.run(list(map(str, command)), cwd=cwd, env=env, check=True)


def validate_compiler_target(actual, target):
    arch = 'aarch64' if target['arch'] == 'arm64' else 'x86_64'
    if not actual.startswith(arch + '-'):
        raise ValueError('Target compiler architecture differs from requested target: ' + actual)
    platform = target['platform']
    if platform == 'windows':
        matches = 'mingw32' in actual or '-windows-gnu' in actual
    elif platform == 'linux':
        matches = '-linux-' in actual and 'android' not in actual
    else:
        raise ValueError('Compiler target validation requires a desktop GNU/LLVM target')
    if not matches:
        raise ValueError('Target compiler operating system/ABI differs from requested target: ' + actual)


def toolchain(target, work, ndk, c_compiler=None, cxx_compiler=None):
    """Return explicit cross tools and flags. Never probe host pkg-config libraries."""
    system = target['platform']
    arch = 'aarch64' if target['arch'] == 'arm64' else 'x86_64'
    identity = {'host': platform.platform(), 'target': target['rust_target'], 'minimum': target['minimum']}
    cc, cxx, ar, ranlib, nm = 'cc', 'c++', 'ar', 'ranlib', 'nm'
    flags, link, configure = [], [], ['--arch=' + arch]
    if system in ('macos', 'ios', 'ios-simulator'):
        if platform.system() != 'Darwin':
            raise ValueError('Apple builds require Xcode on macOS')
        sdk = target['sdk']
        sdk_path = run(['xcrun', '--sdk', sdk, '--show-sdk-path'])
        cc = run(['xcrun', '--sdk', sdk, '--find', 'clang'])
        cxx = run(['xcrun', '--sdk', sdk, '--find', 'clang++'])
        ar = run(['xcrun', '--find', 'ar'])
        ranlib = run(['xcrun', '--find', 'ranlib'])
        nm = run(['xcrun', '--find', 'nm'])
        triple = ('arm64' if arch == 'aarch64' else arch) + '-apple-'
        triple += ('macosx' if system == 'macos' else 'ios') + target['minimum']
        if system == 'ios-simulator':
            triple += '-simulator'
        flags += ['-target', triple, '-isysroot', sdk_path]
        link += flags + ['-Wl,-reproducible']
        configure += ['--target-os=darwin', '--enable-cross-compile']
        identity.update(xcode=run(['xcodebuild', '-version']), sdk=sdk,
                        sdk_version=run(['xcrun', '--sdk', sdk, '--show-sdk-version']))
    elif system == 'android':
        if not ndk:
            raise ValueError('--ndk is required; do not use an arbitrary Android compiler')
        ndk = ndk.resolve()
        properties = (ndk / 'source.properties').read_text()
        if not re.search(r'Pkg.Revision\s*=\s*' + re.escape(target['ndk_revision']) + r'\s*$', properties, re.M):
            raise ValueError('Install the linked specification-pinned Android NDK')
        host = {'Darwin': 'darwin-x86_64', 'Linux': 'linux-x86_64', 'Windows': 'windows-x86_64'}[platform.system()]
        tools = ndk / 'toolchains/llvm/prebuilt' / host / 'bin'
        suffix = '.cmd' if platform.system() == 'Windows' else ''
        cc = str(tools / (target['compiler_prefix'] + '-clang' + suffix))
        cxx = str(tools / (target['compiler_prefix'] + '-clang++' + suffix))
        ar, ranlib, nm = [str(tools / name) for name in ('llvm-ar', 'llvm-ranlib', 'llvm-nm')]
        link += ['-Wl,-z,max-page-size=16384', '-Wl,-z,common-page-size=16384']
        configure += ['--target-os=android', '--enable-cross-compile']
        identity.update(ndk_revision=target['ndk_revision'], android_min_api=int(target['minimum']), elf_max_page_size=16384)
    else:
        if system == 'windows':
            prefix = target['compiler_prefix']
            cc, cxx = c_compiler or prefix + '-clang', cxx_compiler or prefix + '-clang++'
            ar, ranlib, nm = 'llvm-ar', 'llvm-ranlib', 'llvm-nm'
            configure += ['--target-os=mingw32', '--enable-cross-compile']
            link += ['-Wl,--no-insert-timestamp']
        elif platform.system() == 'Linux' and ((arch == 'aarch64') == (platform.machine() in ('arm64', 'aarch64'))):
            cc, cxx = 'gcc', 'g++'
        else:
            prefix = target['compiler_prefix']
            cc, cxx, ar, ranlib, nm = [prefix + '-' + name for name in ('gcc', 'g++', 'ar', 'ranlib', 'nm')]
            configure += ['--target-os=linux', '--enable-cross-compile']
        for name, compiler in [('compiler_target', cc), ('cxx_compiler_target', cxx)]:
            actual = run([compiler, '-dumpmachine'])
            validate_compiler_target(actual, target)
            identity[name] = actual
    for name, executable in [('cc', cc), ('cxx', cxx), ('ar', ar), ('ranlib', ranlib), ('nm', nm)]:
        if shutil.which(executable) is None:
            raise ValueError('Missing target tool: ' + executable)
        try:
            version = run([executable, '--version'], stderr=subprocess.STDOUT).splitlines()[0]
        except subprocess.CalledProcessError:
            version = 'Version is provided by recorded SDK/toolchain identity'
        identity[name] = {'version': version, 'sha256': digest(shutil.which(executable))}
    flags += ['-O2', '-fPIC', '-ffile-prefix-map=' + str(work) + '=/avid-build', '-fdebug-prefix-map=' + str(work) + '=/avid-build']
    if system == 'windows' and arch == 'x86_64':
        flags += ['-fno-builtin-lrintf']
    return {'CC': cc, 'CXX': cxx, 'AR': ar, 'RANLIB': ranlib, 'NM': nm}, flags, link, configure, identity


def compile_dependencies(spec, profile, sources, prefix, work, env, flags, link, target, jobs, cmake, cpp_runtime):
    def install(directory):
        execute(['make', '-j', jobs], directory, env)
        execute(['make', 'install'], directory, env)
    zlib = sources['zlib']
    if target['platform'] == 'windows':
        execute(['make', '-f', 'win32/Makefile.gcc', '-j', jobs, 'libz.a', 'CC=' + env['CC'],
                 'AR=' + env['AR'], 'CFLAGS=' + ' '.join(flags)], zlib, env)
        (prefix / 'include').mkdir(parents=True)
        (prefix / 'lib/pkgconfig').mkdir(parents=True)
        for name in ('zlib.h', 'zconf.h'):
            shutil.copy2(zlib / name, prefix / 'include' / name)
        shutil.copy2(zlib / 'libz.a', prefix / 'lib/libz.a')
        (prefix / 'lib/pkgconfig/zlib.pc').write_text('prefix=${pcfiledir}/../..\nlibdir=${prefix}/lib\nincludedir=${prefix}/include\nName: zlib\nDescription: pinned zlib\nVersion: 1.3.1\nLibs: -L${libdir} -lz\nCflags: -I${includedir}\n')
    else:
        execute(['sh', 'configure', '--static', '--prefix=' + str(prefix)], zlib, env)
        install(zlib)
    if 'x264' in profile['dependencies']:
        # Disable assembly to avoid hidden build-time NASM downloads and to keep
        # every cross target on one portable, explicitly identified source path.
        host = ('aarch64' if target['arch'] == 'arm64' else 'x86_64') + '-'
        host += 'w64-mingw32' if target['platform'] == 'windows' else ('apple-darwin' if 'sdk' in target else 'linux-gnu')
        execute(['sh', 'configure', '--host=' + host, '--prefix=' + str(prefix), '--enable-static', '--disable-cli',
                 '--disable-opencl', '--disable-asm', '--enable-pic'], sources['x264'], env)
        install(sources['x264'])
    if 'x265' in profile['dependencies']:
        # x265's ASM is deliberately disabled for the same target-portable profile.
        toolchain_file = work / 'x265-toolchain.cmake'
        systems = {'macos': 'Darwin', 'ios': 'iOS', 'ios-simulator': 'iOS', 'windows': 'Windows', 'linux': 'Linux', 'android': 'Linux'}
        values = {'CMAKE_SYSTEM_NAME': systems[target['platform']], 'CMAKE_SYSTEM_PROCESSOR': target['arch'],
                  'CMAKE_C_COMPILER': env['CC'], 'CMAKE_CXX_COMPILER': env['CXX'], 'CMAKE_AR': env['AR'],
                  'CMAKE_RANLIB': env['RANLIB'], 'CMAKE_C_FLAGS_INIT': ' '.join(flags),
                  'CMAKE_CXX_FLAGS_INIT': env['CXXFLAGS'], 'CMAKE_EXE_LINKER_FLAGS_INIT': ' '.join(link),
                  'CMAKE_TRY_COMPILE_TARGET_TYPE': 'STATIC_LIBRARY'}
        # Apple target/sysroot are explicit in C/C++ flags; avoid CMake selecting a host SDK.
        if 'sdk' in target:
            values['CMAKE_OSX_SYSROOT'] = run(['xcrun', '--sdk', target['sdk'], '--show-sdk-path'])
            values['CMAKE_OSX_ARCHITECTURES'] = target['arch']
            values['CMAKE_OSX_DEPLOYMENT_TARGET'] = target['minimum']
        toolchain_file.write_text(''.join('set(' + key + ' "' + value.replace('\\', '/') + '")\n' for key, value in values.items()))
        xbuild = work / 'x265-build'
        execute([cmake, '-S', sources['x265'] / 'source', '-B', xbuild, '-G', 'Unix Makefiles',
                 '-DCMAKE_TOOLCHAIN_FILE=' + str(toolchain_file), '-DCMAKE_BUILD_TYPE=Release',
                 '-DCMAKE_INSTALL_PREFIX=' + str(prefix), '-DCMAKE_INSTALL_LIBDIR=lib', '-DENABLE_SHARED=OFF',
                 '-DENABLE_CLI=OFF', '-DENABLE_ASSEMBLY=OFF', '-DENABLE_LIBNUMA=OFF', '-DCMAKE_POSITION_INDEPENDENT_CODE=ON',
                 '-DCMAKE_POLICY_VERSION_MINIMUM=3.5'], work, env)
        execute([cmake, '--build', xbuild, '--parallel', jobs], work, env)
        execute([cmake, '--install', xbuild], work, env)
        # Cross CMake has no implicit C++ link probe, so x265 omits its C++
        # runtime in generated pkg-config metadata. Preserve the explicit target
        # runtime dependency; FFmpeg's C configure probe needs it as well.
        runtime = cpp_runtime
        pc = prefix / 'lib/pkgconfig/x265.pc'
        text = pc.read_text()
        text = re.sub(r'^(Libs.private:.*)$', lambda match: match[1] + ' -l' + runtime, text, flags=re.M)
        pc.write_text(text)


def build(args):
    spec_hash = digest(SPEC_PATH)
    spec = json.loads(SPEC_PATH.read_text())
    target = next(t for t in spec['targets'] if t['id'] == args.target)
    profile = spec['profiles'][args.profile]
    if target['platform'] not in profile['platforms']:
        raise ValueError('Profile does not support this target platform')
    cpp_runtime = cxx_runtime(target, args.cxx_runtime)
    cmake = str(args.cmake)
    if 'x265' in profile['dependencies'] and run([cmake, '--version']).splitlines()[0] != 'cmake version 3.31.6':
        raise ValueError('x265 requires pinned CMake 3.31.6; provide --cmake /path/to/cmake')
    work = args.work.resolve()
    if any(c.isspace() for c in str(work)):
        raise ValueError('Upstream configure requires a work path without whitespace')
    # Validate prerequisites before creating the fresh work directory.
    tools, flags, link, cross, identity = toolchain(target, work, args.ndk, args.cc, args.cxx)
    cache = args.cache.resolve()
    provenance = authenticate_ffmpeg(spec, cache, args.fetch)
    archives = {'ffmpeg': acquire('ffmpeg', spec['ffmpeg'], cache, args.fetch)}
    for name in profile['dependencies']:
        archives[name] = acquire(name, spec['dependencies'][name], cache, args.fetch)
    work.mkdir(parents=True, exist_ok=False)
    prefix = work / 'prefix'
    prefix.mkdir()
    sources = {name: extract(path, work / name, strip_root=name != 'x264') for name, path in archives.items()}
    # A whitelist avoids accidental SDK/library flags and development machine paths.
    env = {key: value for key, value in os.environ.items() if key in ('PATH', 'HOME', 'SystemRoot', 'WINDIR', 'TEMP', 'TMP', 'TMPDIR')}
    env.update(tools, SOURCE_DATE_EPOCH=str(spec['source_date_epoch']), TZ='UTC', LC_ALL='C', ZERO_AR_DATE='1',
               PKG_CONFIG_PATH='', PKG_CONFIG_LIBDIR=str(prefix / 'lib/pkgconfig'),
               CFLAGS=' '.join(flags), CXXFLAGS=' '.join(flags + (['-stdlib=' + ('libc++' if cpp_runtime == 'c++' else 'libstdc++')] if target['platform'] == 'windows' else [])), CPPFLAGS='-I' + str(prefix / 'include'),
               LDFLAGS=' '.join(link + ['-L' + str(prefix / 'lib')]))
    compile_dependencies(spec, profile, sources, prefix, work, env, flags, link, target, args.jobs, cmake, cpp_runtime)
    encoders = spec['encoders'] + profile['encoders']
    configure = spec['configure'] + profile['configure'] + cross + ['--prefix=' + str(prefix), '--pkg-config-flags=--static',
        '--cc=' + tools['CC'], '--cxx=' + tools['CXX'], '--ar=' + tools['AR'], '--ranlib=' + tools['RANLIB'], '--nm=' + tools['NM'],
        '--disable-x86asm', '--enable-encoder=' + ','.join(encoders), '--enable-filter=' + ','.join(spec['filters']),
        '--extra-cflags=' + env['CPPFLAGS'] + ' ' + env['CFLAGS'], '--extra-ldflags=' + env['LDFLAGS']]
    ffmpeg = sources['ffmpeg']
    execute(['sh', 'configure', *configure], ffmpeg, env)
    execute(['make', '-j', args.jobs], ffmpeg, env)
    execute(['make', 'install'], ffmpeg, env)
    # CMake may normalize /private/tmp to /tmp on macOS. Rewrite the
    # generated prefix assignment itself rather than relying on string identity.
    for pc in (prefix / 'lib/pkgconfig').glob('*.pc'):
        text = re.sub(r'^prefix=.*$', 'prefix=${pcfiledir}/../..', pc.read_text(), flags=re.M)
        pc.write_text(text.replace(str(prefix), '${pcfiledir}/../..'))
    components = (ffmpeg / 'config_components.h').read_text()
    enabled = {kind: sorted(name.lower() for name in re.findall(r'#define CONFIG_(\w+)_' + kind.upper() + r' 1', components))
               for kind in ('encoder', 'decoder', 'filter', 'muxer', 'demuxer', 'protocol')}
    for name in encoders:
        if name not in enabled['encoder']:
            raise ValueError('Requested encoder did not compile: ' + name)
    if enabled['protocol'] != ['file']:
        raise ValueError('Unexpected media protocol compiled into linked profile')
    from binary import validate_file
    for library in spec['libraries']:
        validate_file(prefix / 'lib' / ('lib' + library + '.a'), target)
    licenses = prefix / 'licenses'
    for name, directory in sources.items():
        destination = licenses / name
        destination.mkdir(parents=True)
        for path in directory.iterdir():
            if path.is_file() and path.name.startswith(('COPYING', 'LICENSE', 'LICENCE')):
                shutil.copy2(path, destination / path.name)
        if name == 'zlib':
            shutil.copy2(directory / 'README', destination / 'README')
    identity.update(make=run(['make', '--version']).splitlines()[0], cmake=run([cmake, '--version']).splitlines()[0],
                    pkg_config=run(['pkg-config', '--version']), python=platform.python_version())
    if digest(SPEC_PATH) != spec_hash:
        raise ValueError('Linked specification changed during build; discard candidate')
    runtime_dependencies = android_runtime(args.ndk, target, prefix) if target['platform'] == 'android' else {}
    metadata = {'schema': 1, 'kind': 'avid-linked-ffmpeg', 'artifact_version': spec['artifact_version'],
                'target': target['rust_target'], 'target_id': args.target, 'platform': target['platform'], 'arch': target['arch'],
                'profile': args.profile, 'minimum': target['minimum'], 'linkage': 'static',
                'ffmpeg_version': spec['ffmpeg']['version'], 'source_revision': spec['ffmpeg']['revision'],
                'source_provenance': provenance, 'spec_sha256': digest(SPEC_PATH), 'toolchain': identity,
                'configure': [value.replace(str(work), '/avid-build') for value in configure],
                'components': enabled, 'sources': {name: {'archive': path.name, 'sha256': digest(path)} for name, path in archives.items()},
                'libraries': {path.relative_to(prefix).as_posix(): digest(path) for path in sorted((prefix / 'lib').glob('*.a'))},
                'validation': {'archive_architecture': 'passed', 'native_runtime': 'not_run'},
                'ffmpeg_license': profile['ffmpeg_license'], 'cxx_runtime': cpp_runtime, 'runtime_dependencies': runtime_dependencies}
    (prefix / 'linked-ffmpeg.json').write_text(json.dumps(metadata, indent=2) + '\n')
    # Corresponding pristine source archives and exact build recipe are retained.
    output_sources = prefix / 'corresponding-source'
    output_sources.mkdir()
    for path in archives.values():
        shutil.copy2(path, output_sources / path.name)
    shutil.copy2(SPEC_PATH, output_sources / 'linked-spec.json')
    shutil.copytree(ROOT / 'scripts/linked', output_sources / 'scripts', ignore=shutil.ignore_patterns('__pycache__'))
    print('Built linked libraries (native execution not yet qualified):', prefix)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target', required=True)
    parser.add_argument('--profile', required=True, choices=['apple-lgpl', 'gpl-software', 'apple-gpl'])
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--cache', type=Path, default=ROOT / '.ffmpeg-work/downloads')
    parser.add_argument('--ndk', type=Path)
    parser.add_argument('--cxx-runtime', choices=['c++', 'stdc++', 'c++_shared'])
    parser.add_argument('--cc', help='Explicit target C driver (Windows GNU/LLVM)')
    parser.add_argument('--cxx', help='Explicit target C++ driver (Windows GNU/LLVM)')
    parser.add_argument('--cmake', type=Path, default=Path('cmake'), help='CMake 3.31.6 for the GPL software profile')
    parser.add_argument('--fetch', action='store_true', help='Deliberately acquire pinned sources/authentication inputs over HTTPS')
    parser.add_argument('--jobs', type=int, default=min(os.cpu_count() or 2, 8))
    build(parser.parse_args())
