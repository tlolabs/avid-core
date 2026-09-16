#!/usr/bin/env python3
"""Build the manifest's source runtime. No package-manager media libraries are used."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import gzip

ROOT = Path(__file__).resolve().parents[2]
SPEC_PATH = ROOT / 'runtime/ffmpeg/spec.json'


def digest(path):
    with open(path, 'rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def run(args, cwd, env):
    print('+', ' '.join(map(str, args)), flush=True)
    subprocess.run(list(map(str, args)), cwd=cwd, env=env, check=True)


def download(item, cache):
    dest = cache / item['url'].rsplit('/', 1)[1]
    if not dest.exists() or digest(dest) != item['sha256']:
        temporary = dest.with_suffix(dest.suffix + '.partial')
        subprocess.run(['curl', '--fail', '--location', '--retry', '3', '--proto', '=https',
                        '--proto-redir', '=https', '--output', str(temporary), item['url']], check=True)
        if digest(temporary) != item['sha256']:
            raise ValueError(f'Source checksum mismatch: {item["url"]}')
        temporary.replace(dest)
    return dest


def source(name, item, cache, directory, env):
    dest = directory / name
    if dest.exists():
        raise ValueError(f'Fresh source directory required: {dest}')
    if 'git' in item and 'url' not in item:
        # Cache immutable git objects, never a working tree or compiled output.
        bare = cache / (name + '.git')
        if not bare.exists():
            run(['git', 'init', '--bare', bare], directory, env)
        run(['git', '-C', bare, 'fetch', '--depth=1', item['git'], item['revision']], directory, env)
        actual = subprocess.check_output(['git', '-C', str(bare), 'rev-parse', 'FETCH_HEAD'], text=True).strip()
        if actual != item['revision']:
            raise ValueError('Git revision mismatch')
        archive = cache / (name + '-' + actual + '.tar')
        with archive.open('wb') as f:
            subprocess.run(['git', '-C', str(bare), 'archive', actual, *item.get('source_paths', [])], stdout=f, check=True)
        dest.mkdir()
        with tarfile.open(archive) as tar:
            tar.extractall(dest, filter='data')
    else:
        archive = download(item, cache)
        dest.mkdir()
        with tarfile.open(archive) as tar:
            tar.extractall(dest, filter='data')
        children = list(dest.iterdir())
        if len(children) != 1 or not children[0].is_dir():
            raise ValueError(f'Unexpected archive layout: {name}')
        dest = children[0]
    return dest


def input_snapshot():
    names = subprocess.check_output(['git', '-C', str(ROOT), 'ls-files', '-co', '--exclude-standard', '-z']).decode().split('\0')
    return {
        'revision': subprocess.check_output(['git', '-C', str(ROOT), 'rev-parse', 'HEAD']).decode().strip(),
        'files': {name: digest(ROOT/name) if (ROOT/name).is_file() else None for name in sorted(set(names)) if name},
    }


def build(args):
    initial_inputs = input_snapshot()
    spec = json.loads(SPEC_PATH.read_text())
    target = next(t for t in spec['targets'] if t['id'] == args.target)
    work = args.work.resolve()
    if any(c.isspace() for c in str(work)):
        raise ValueError('Build directory must not contain spaces (upstream configure limitation)')
    # Refuse reuse; caller chooses a fresh directory for every full build.
    work.mkdir(parents=True, exist_ok=False)
    (work/'.avid-build-root').write_text(args.target)
    cache = args.cache.absolute()
    cache.mkdir(parents=True, exist_ok=True)
    prefix = work / 'prefix'
    prefix.mkdir()
    env = {k: v for k, v in os.environ.items() if k not in
           ('CFLAGS', 'CXXFLAGS', 'CPPFLAGS', 'LDFLAGS', 'CPATH', 'LIBRARY_PATH', 'C_INCLUDE_PATH',
            'CPLUS_INCLUDE_PATH', 'PKG_CONFIG_PATH', 'PKG_CONFIG_LIBDIR', 'SDKROOT', 'CC', 'CXX')}
    env.update(SOURCE_DATE_EPOCH=str(spec['source_date_epoch']), TZ='UTC', LC_ALL='C',
               ZERO_AR_DATE='1', PKG_CONFIG_PATH='', PKG_CONFIG_LIBDIR=str(prefix / 'lib/pkgconfig'))
    system = target['os']
    actual_os = platform.system().lower()
    if system == 'macos' and actual_os != 'darwin' or system == 'linux' and actual_os != 'linux':
        raise ValueError('Build target OS must match the native environment')
    if system == 'windows' and not (actual_os.startswith(('windows', 'msys', 'mingw', 'cygwin'))):
        raise ValueError('Windows builds require the native MSYS2 environment')
    machine = platform.machine().lower()
    if system != 'windows' and ('arm64' if machine in ('arm64', 'aarch64') else machine) != target['arch']:
        raise ValueError('Native target architecture does not match this machine')
    if system == 'macos':
        env['MACOSX_DEPLOYMENT_TARGET'] = '13.0'
        env['CC'], env['CXX'] = 'clang', 'clang++'
    elif system == 'windows':
        env['CC'], env['CXX'] = 'clang', 'clang++'
    else:
        env['CC'], env['CXX'] = 'gcc', 'g++'
    flags = f'-O2 -ffile-prefix-map={work}=/avid-build -fdebug-prefix-map={work}=/avid-build'
    env.update(CFLAGS=flags, CXXFLAGS=flags, CPPFLAGS=f'-I{prefix}/include', LDFLAGS=f'-L{prefix}/lib')
    if system == 'macos':
        env['LDFLAGS'] += ' -Wl,-reproducible'
    if system == 'windows':
        env['LDFLAGS'] += ' -static -Wl,--no-insert-timestamp'
        env.update(AR='llvm-ar', RANLIB='llvm-ranlib', NM='llvm-nm', STRIP='llvm-strip')
    env['PATH'] = str(prefix / 'bin') + os.pathsep + env['PATH']
    meta = {'schema': 1, 'target': args.target, 'spec_sha256': digest(SPEC_PATH),
            'version': spec['source']['version'], 'source_revision': spec['source']['revision'],
            'recipe': spec['recipe'], 'platform': platform.platform(),
            'environment': {k: env[k] for k in ['CC', 'CXX', 'CFLAGS', 'CXXFLAGS', 'LDFLAGS', 'SOURCE_DATE_EPOCH']},
            'runner_image': os.environ.get('ImageVersion'), 'ci_run': os.environ.get('GITHUB_RUN_ID'),
            'core_revision': subprocess.check_output(['git', '-C', str(ROOT), 'rev-parse', 'HEAD'], text=True).strip(),
            'core_worktree_modified': bool(subprocess.check_output(['git', '-C', str(ROOT), 'status', '--porcelain'], text=True).strip()),
            'tools': {}}
    for tool in (env['CC'], env['CXX'], 'cmake', 'make', 'pkg-config', 'python3', 'gpg', 'git'):
        meta['tools'][tool] = subprocess.check_output([tool, '--version'], text=True).splitlines()[0]
    if not meta['tools']['cmake'].endswith(spec['build_tools']['cmake']):
        raise ValueError('Install the manifest-pinned CMake version; x265 is not compatible with CMake 4')
    if system == 'linux':
        meta['system_packages'] = subprocess.check_output(['dpkg-query','-W','-f=${Package}=${Version}\n'],text=True).splitlines()
    elif system == 'windows':
        meta['system_packages'] = subprocess.check_output(['pacman','-Q'],text=True).splitlines()
        meta['compiler_target'] = subprocess.check_output([env['CC'],'-dumpmachine'],text=True).strip()
        expected_arch = 'aarch64' if target['arch']=='arm64' else 'x86_64'
        if not meta['compiler_target'].startswith(expected_arch+'-'):
            raise ValueError('Windows compiler architecture differs from native target')
    if system == 'macos':
        meta['xcode'] = subprocess.check_output(['xcodebuild','-version'],text=True).strip()
        meta['sdk'] = subprocess.check_output(['xcrun', '--show-sdk-version'], text=True).strip()
    from provenance import verify
    source_provenance = verify(cache)
    deps = {name: source(name, item, cache, work, env) for name, item in spec['dependencies'].items() if name in target['dependencies']}
    ff = source('ffmpeg', spec['source'], cache, work, env)
    if (ff / 'VERSION').read_text().strip() != spec['source']['version']:
        raise ValueError('Archive VERSION differs from manifest')
    def make_install(cwd):
        run(['make', '-j', args.jobs], cwd, env)
        run(['make', 'install'], cwd, env)
    if system == 'windows':
        # zlib's configure explicitly rejects MinGW; build only its static target.
        run(['make', '-f', 'win32/Makefile.gcc', '-j', args.jobs, 'libz.a',
             'CC=clang', 'AR=llvm-ar', 'CFLAGS=' + flags], deps['zlib'], env)
        (prefix / 'include').mkdir(exist_ok=True)
        (prefix / 'lib/pkgconfig').mkdir(parents=True, exist_ok=True)
        for name in ['zlib.h', 'zconf.h']:
            shutil.copy2(deps['zlib'] / name, prefix / 'include' / name)
        shutil.copy2(deps['zlib'] / 'libz.a', prefix / 'lib/libz.a')
        pc = (deps['zlib'] / 'zlib.pc.in').read_text()
        for name, value in {'prefix': str(prefix), 'exec_prefix': str(prefix),
                            'libdir': str(prefix / 'lib'), 'sharedlibdir': str(prefix / 'lib'),
                            'includedir': str(prefix / 'include'),
                            'VERSION': spec['dependencies']['zlib']['version']}.items():
            pc = pc.replace('@' + name + '@', value)
        (prefix / 'lib/pkgconfig/zlib.pc').write_text(pc)
    else:
        run(['sh', 'configure', '--prefix=' + str(prefix), '--static'], deps['zlib'], env)
        make_install(deps['zlib'])
    if target['arch'] == 'x86_64':
        run(['sh', 'configure', '--prefix=' + str(prefix)], deps['nasm'], env)
        make_install(deps['nasm'])
    host = (['--host=' + ('aarch64' if target['arch'] == 'arm64' else 'x86_64') + '-w64-mingw32'] if system == 'windows' else [])
    run(['bash', 'configure', *host, '--prefix=' + str(prefix), '--enable-static', '--disable-cli',
         '--disable-opencl', '--enable-pic'], deps['x264'], env)
    make_install(deps['x264'])
    if system == 'windows':
        # CMake 3.31 reports LLVM's unwind runtime as -l:libunwind.a. x265
        # incorrectly adds another -l when creating its static pkg-config file.
        cmake_source = deps['x265']/'source/CMakeLists.txt'
        old = 'list(APPEND PLIBLIST "${LIB}")\n        else()'
        replacement = 'list(APPEND PLIBLIST "${LIB}")\n        elseif(LIB MATCHES "^-l")\n            list(APPEND PLIBLIST "${LIB}")\n        else()'
        content = cmake_source.read_text()
        if content.count(old) != 1:
            raise ValueError('Pinned x265 LLVM pkg-config patch no longer applies')
        cmake_source.write_text(content.replace(old, replacement))
    xbuild = work / 'x265-build'
    cmake = ['cmake', '-S', deps['x265'] / 'source', '-B', xbuild, '-G', ('MSYS Makefiles' if system == 'windows' else 'Unix Makefiles'),
             '-DCMAKE_BUILD_TYPE=Release',
             '-DCMAKE_INSTALL_PREFIX=' + str(prefix), '-DCMAKE_INSTALL_LIBDIR=lib',
             '-DENABLE_SHARED=OFF', '-DENABLE_CLI=OFF', '-DENABLE_LIBNUMA=OFF',
             '-DENABLE_ASSEMBLY=ON',
             '-DCMAKE_POSITION_INDEPENDENT_CODE=ON']
    run(cmake, work, env)
    run(['cmake', '--build', xbuild, '--parallel', args.jobs], work, env)
    run(['cmake', '--install', xbuild], work, env)
    run(['sh', 'configure', *host, '--prefix=' + str(prefix), '--disable-shared', '--enable-static',
         '--disable-frontend', '--disable-decoder', '--with-pic'], deps['lame'], env)
    make_install(deps['lame'])
    encoders = spec['encoders'] + target['required_encoders'] + target.get('optional_encoders', [])
    configure = list(spec['configure']) + ['--prefix=' + str(prefix), '--pkg-config-flags=--static',
                 '--extra-cflags=' + env['CPPFLAGS'] + ' ' + flags,
                 '--extra-ldflags=' + env['LDFLAGS'], '--cc=' + env['CC'], '--cxx=' + env['CXX'],
                 '--enable-encoder=' + ','.join(encoders), '--enable-filter=' + ','.join(spec['filters'])]
    if system == 'macos':
        configure += ['--enable-videotoolbox', '--enable-audiotoolbox']
    if system == 'windows':
        configure += ['--ar=llvm-ar', '--ranlib=llvm-ranlib', '--nm=llvm-nm', '--strip=llvm-strip', '--windres=llvm-windres', '--target-os=mingw32', '--arch=' + ('aarch64' if target['arch'] == 'arm64' else 'x86_64')]
    try:
        run(['sh', 'configure', *configure], ff, env)
    except subprocess.CalledProcessError:
        print((ff/'ffbuild/config.log').read_text(errors='replace')[-16000:], flush=True)
        raise
    executable_suffix = '.exe' if system == 'windows' else ''
    run(['make', '-j', args.jobs, 'ffmpeg' + executable_suffix, 'ffprobe' + executable_suffix], ff, env)
    meta['configure'] = configure
    meta['build_scripts_sha256'] = {p.name: digest(p) for p in sorted((ROOT / 'scripts/ffmpeg').glob('*.py'))}
    import re
    components = (ff / 'config_components.h').read_text()
    meta['parsers'] = sorted(n.lower() for n in re.findall(r'#define CONFIG_(\w+)_PARSER 1', components))
    package = args.output.absolute() / artifact_name(spec, args.target)
    package.mkdir(parents=True, exist_ok=False)
    suffix = '.exe' if system == 'windows' else ''
    for tool in ['ffmpeg', 'ffprobe']:
        shutil.copy2(ff / (tool + suffix), package / (tool + suffix))
        if system == 'macos':
            from macho import normalize
            normalize(package / tool)
            meta['macos_uuid'] = 'First 16 SHA-256 bytes of stripped unsigned Mach-O with LC_UUID zeroed; then deterministic ad-hoc signing'
    shutil.copy2(SPEC_PATH, package / 'spec.json')
    (package / 'source-provenance.json').write_text(json.dumps(source_provenance, indent=2)+'\n')
    (package / 'build.json').write_text(json.dumps(meta, indent=2) + '\n')
    licenses = package / 'licenses'
    licenses.mkdir()
    for name, src in dict(deps, ffmpeg=ff).items():
        out = licenses / name
        out.mkdir()
        for path in src.iterdir():
            if path.is_file() and path.name.startswith(('COPYING', 'LICENSE', 'LICENCE')):
                shutil.copy2(path, out / path.name)
    shutil.copy2(deps['zlib'] / 'README', licenses / 'zlib/README')
    shutil.copy2(deps['zlib'] / 'zlib.h', licenses / 'zlib/zlib.h')
    # Exact corresponding source + scripts travel with the candidate set.
    def normalize(info):
        info.uid = info.gid = 0
        info.uname = info.gname = ''
        info.mtime = spec['source_date_epoch']
        return info
    sources = args.output.absolute() / (artifact_name(spec, args.target) + '-sources.tar.gz')
    with sources.open('wb') as raw, gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=spec['source_date_epoch']) as compressed, tarfile.open(fileobj=compressed, mode='w') as archive:
        for name, src in dict(deps, ffmpeg=ff).items():
            # Use pristine archives/git exports rather than compiled source trees.
            item = spec['source'] if name == 'ffmpeg' else spec['dependencies'][name]
            filename = item['url'].rsplit('/', 1)[1] if 'url' in item else name + '-' + item['revision'] + '.tar'
            archive.add(cache / filename, arcname='sources/' + filename, filter=normalize)
        archive.add(ROOT / 'scripts/ffmpeg', arcname='scripts/ffmpeg', filter=lambda i: None if '__pycache__' in i.name else normalize(i))
        archive.add(ROOT/'runtime/ffmpeg', arcname='runtime/ffmpeg', filter=normalize)
        archive.add(ROOT/'docs/ffmpeg/licensing.md', arcname='docs/ffmpeg/licensing.md', filter=normalize)
        for name in ['ffmpeg-release.asc','ffmpeg-release-key.asc','ffmpeg-tag-key.asc']:
            archive.add(cache/name, arcname='provenance/'+name, filter=normalize)
        for name in ['Cargo.toml', 'Cargo.lock', 'LICENSE', 'README.md', 'src', 'tests']:
            archive.add(ROOT / name, arcname=name, filter=normalize)
    source_info = {'repository':spec['release_repository'], 'tag':f'ffmpeg-{spec["source"]["version"]}-r{spec["recipe"]}', 'asset':sources.name, 'sha256':digest(sources)}
    (package/'SOURCE.json').write_text(json.dumps(source_info,indent=2)+'\n')
    shutil.copy2(ROOT/'docs/ffmpeg/licensing.md',package/'licenses/REDISTRIBUTION.md')
    if input_snapshot() != initial_inputs:
        raise ValueError('Core build inputs changed during compilation; discard this attempt and rebuild a fixed checkout')
    print('Built candidate:', package, flush=True)


def artifact_name(spec, target):
    return f'avid-ffmpeg-{spec["source"]["version"]}-r{spec["recipe"]}-{target}'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target', required=True)
    parser.add_argument('--work', type=Path, required=True, help='Fresh directory without spaces')
    parser.add_argument('--cache', type=Path, default=ROOT / '.ffmpeg-work/downloads')
    parser.add_argument('--output', type=Path, default=ROOT / 'dist')
    parser.add_argument('--jobs', type=int, default=min(os.cpu_count() or 2, 8))
    build(parser.parse_args())
