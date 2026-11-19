#!/usr/bin/env python3
"""Pinned source acquisition for deliberate builds; never imported by the media runtime."""
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[2]
SPEC_PATH = ROOT / 'runtime/linked/spec.json'


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def run(command, **kwargs):
    return subprocess.check_output(list(map(str, command)), text=True, **kwargs).strip()


def archive_path(name, item, cache):
    filename = item['url'].rsplit('/', 1)[1] if 'url' in item else name + '-' + item['revision'] + '.tar'
    return cache / filename


def acquire(name, item, cache, fetch=False):
    cache.mkdir(parents=True, exist_ok=True)
    archive = archive_path(name, item, cache)
    expected = item.get('sha256', item.get('archive_sha256'))
    if archive.exists() and digest(archive) == expected:
        return archive
    if not fetch:
        raise ValueError(f'Missing or invalid pinned source {archive}; use --fetch for deliberate acquisition')
    with tempfile.TemporaryDirectory(prefix='.avid-acquire-', dir=cache) as staging:
        temporary = Path(staging) / archive.name
        if 'url' in item:
            # Fixed URLs only, redirects must remain HTTPS.
            subprocess.run(['curl', '--fail', '--location', '--proto', '=https', '--proto-redir', '=https',
                            '--output', str(temporary), item['url']], check=True)
        else:
            repo = cache / (name + '.git')
            if not repo.exists():
                run(['git', 'init', '--bare', repo])
            run(['git', '-C', repo, 'fetch', '--depth=1', item['git'], item['revision']])
            if run(['git', '-C', repo, 'rev-parse', 'FETCH_HEAD']) != item['revision']:
                raise ValueError('Dependency git identity mismatch')
            with temporary.open('wb') as stream:
                subprocess.run(['git', '-C', str(repo), 'archive', item['revision']], stdout=stream, check=True)
        if digest(temporary) != expected:
            raise ValueError(f'Source checksum mismatch for {name}')
        try:
            temporary.replace(archive)
        except PermissionError:
            # Another builder may already be reading the same pinned archive.
            if not archive.is_file() or digest(archive) != expected:
                raise
    return archive


def extract(archive, destination, strip_root=True):
    def validate_member(member, directory):
        parts = PurePosixPath(member.name).parts
        if member.name.startswith('/') or '..' in parts or member.isdev() or member.isfifo():
            raise ValueError('Unsafe source archive member')
        return tarfile.data_filter(member, directory)

    destination.mkdir(parents=True, exist_ok=False)
    try:
        # Validate during extraction to decompress only once. Keep the payload
        # private until all members and the requested root layout have passed.
        with tempfile.TemporaryDirectory(prefix='.avid-extract-', dir=destination.parent) as temporary:
            staging = Path(temporary) / destination.name
            staging.mkdir()
            with tarfile.open(archive) as source:
                source.extractall(staging, filter=validate_member)
            entries = list(staging.iterdir())
            if strip_root and (len(entries) != 1 or not entries[0].is_dir()):
                raise ValueError('Expected a single source archive root')
            for entry in entries:
                entry.rename(destination / entry.name)
    except BaseException:
        shutil.rmtree(destination)
        raise
    return destination / entries[0].name if strip_root else destination


def signature(status, fingerprint):
    invalid = ('BADSIG', 'ERRSIG', 'EXPSIG', 'EXPKEYSIG', 'REVKEYSIG', 'KEYEXPIRED', 'SIGEXPIRED')
    if any('[GNUPG:] ' + word in status for word in invalid):
        raise ValueError('Invalid source signature')
    fingerprints = [line.split()[2] for line in status.splitlines() if line.startswith('[GNUPG:] VALIDSIG ')]
    if fingerprints != [fingerprint]:
        raise ValueError('Source signature identity mismatch')
    return fingerprints[0]


def tree(archive, prefix=''):
    files = {}
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        for member in tar:
            if member.isdir():
                continue
            path = member.name.removeprefix(prefix)
            if path in files or path.startswith('/') or '..' in PurePosixPath(path).parts:
                raise ValueError('Unsafe/duplicate source member')
            if member.isfile():
                files[path] = {'sha256': hashlib.sha256(tar.extractfile(member).read()).hexdigest(),
                               'executable': bool(member.mode & 0o111)}
            elif member.issym():
                files[path] = {'symlink': member.linkname}
            else:
                raise ValueError('Unsupported archive member')
    return files


def authenticate_ffmpeg(spec, cache, fetch=False):
    """Retain the executable recipe's signed archive AND signed tag verification."""
    source = spec['ffmpeg']
    archive = acquire('ffmpeg', source, cache, fetch)
    certificate = ROOT / source['tag_key_certificate']['path']
    if digest(certificate) != source['tag_key_certificate']['sha256']:
        raise ValueError('Pinned FFmpeg tag certificate differs')
    release_key = cache / 'ffmpeg-release-key.asc'
    release_signature = cache / 'ffmpeg-release.asc'
    repo = cache / 'ffmpeg-provenance.git'
    if fetch:
        for dest, url in [(release_key, 'https://ffmpeg.org/ffmpeg-devel.asc'),
                          (release_signature, source['url'] + '.asc')]:
            subprocess.run(['curl', '-fsSL', '--proto', '=https', '--proto-redir', '=https', '-o', str(dest), url], check=True)
        if not repo.exists():
            run(['git', 'init', '--bare', repo])
        run(['git', '-C', repo, 'fetch', '--force', '--depth=1', source['git'],
             'refs/tags/' + source['tag'] + ':refs/tags/' + source['tag']])
    if not release_key.is_file() or not release_signature.is_file() or not repo.is_dir():
        raise ValueError('Missing source authentication inputs; run deliberate --fetch once')
    with tempfile.TemporaryDirectory(prefix='avid-linked-keys-') as temporary:
        (Path(temporary) / 'gpg.conf').write_text('no-autostart\n')
        env = dict(os.environ, GNUPGHOME=temporary)
        for key in [release_key, certificate]:
            run(['gpg', '--batch', '--import', key], env=env, stderr=subprocess.STDOUT)
        status = run(['gpg', '--batch', '--status-fd=1', '--verify', release_signature, archive], env=env, stderr=subprocess.STDOUT)
        signature(status, source['release_key_fingerprint'])
        revision = run(['git', '-C', repo, 'rev-parse', source['tag'] + '^{}'])
        if revision != source['revision']:
            raise ValueError('Signed FFmpeg tag revision differs')
        tag_status = run(['git', '-C', repo, '-c', 'gpg.program=gpg', 'verify-tag', '--raw', source['tag']],
                         env=env, stderr=subprocess.STDOUT)
        signature(tag_status, source['tag_key_fingerprint'])
    actual = tree(archive.read_bytes(), 'ffmpeg-' + source['version'] + '/')
    upstream = tree(subprocess.check_output(['git', '-C', str(repo), 'archive', revision]))
    for name in list(upstream):
        if PurePosixPath(name).name.startswith('.git'):
            del upstream[name]
    if actual.pop('VERSION', None) != {'sha256': hashlib.sha256((source['version'] + '\n').encode()).hexdigest(), 'executable': False}:
        raise ValueError('Archive VERSION differs')
    if actual != upstream:
        raise ValueError('Signed source tree differs from release archive')
    return {'schema': 1, 'status': 'verified', 'source_revision': revision,
            'archive_sha256': digest(archive), 'tree_sha256': hashlib.sha256(json.dumps(upstream, sort_keys=True).encode()).hexdigest(),
            'release_key_fingerprint': source['release_key_fingerprint'],
            'tag_key_fingerprint': source['tag_key_fingerprint'], 'files': len(upstream)}
