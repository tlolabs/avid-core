#!/usr/bin/env python3
"""Authenticate upstream release bytes and compare every tracked source file to its signed tag."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import subprocess
import tarfile
import tempfile
from build import ROOT, SPEC_PATH, download, digest


def checked(args, env=None):
    return subprocess.check_output(list(map(str, args)), env=env, stderr=subprocess.STDOUT).decode()


def valid_signature(status, fingerprint):
    fingerprints = [line.split()[2] for line in status.splitlines() if line.startswith('[GNUPG:] VALIDSIG ')]
    if fingerprints != [fingerprint]:
        raise ValueError('Signature does not match the single pinned signing fingerprint')


def tree(archive, prefix=''):
    result = {}
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        for member in tar:
            if member.isdir():
                continue
            path = member.name.removeprefix(prefix)
            if not path or path.startswith('/') or '..' in PurePosixPath(path).parts or path in result:
                raise ValueError('Unsafe or duplicate source archive member')
            if member.isfile():
                result[path] = {'sha256': hashlib.sha256(tar.extractfile(member).read()).hexdigest(), 'executable': bool(member.mode & 0o111)}
            elif member.issym():
                result[path] = {'symlink': member.linkname}
            else:
                raise ValueError('Unexpected source archive member type')
    return result


def verify(cache):
    spec = json.loads(SPEC_PATH.read_text())
    source = spec['source']
    cache.mkdir(parents=True, exist_ok=True)
    archive = download(source, cache)
    with tempfile.TemporaryDirectory(prefix='avid-provenance-') as temporary:
        work = Path(temporary)
        keyring = work/'keys'
        keyring.mkdir(mode=0o700)
        env = dict(os.environ, GNUPGHOME=str(keyring))
        for name, url in [('release-key.asc', 'https://ffmpeg.org/ffmpeg-devel.asc'),
                          ('release.asc', source['url']+'.asc')]:
            checked(['curl', '-fsSL', '--proto', '=https', '--proto-redir', '=https', '-o', work/name, url])
        certificate = source['tag_key_certificate']
        certificate_path = ROOT/certificate['path']
        if digest(certificate_path) != certificate['sha256']:
            raise ValueError('Tag signing certificate checksum mismatch')
        (work/'tag-key.asc').write_bytes(certificate_path.read_bytes())
        for name in ['release-key.asc', 'tag-key.asc']:
            checked(['gpg', '--batch', '--import', work/name], env)
        status = checked(['gpg', '--batch', '--status-fd=1', '--verify', work/'release.asc', archive], env)
        valid_signature(status, source['release_key_fingerprint'])
        repo = cache/'ffmpeg-provenance.git'
        if not repo.exists():
            checked(['git', 'init', '--bare', repo])
        checked(['git', '-C', repo, 'fetch', '--force', '--depth=1', source['git'], 'refs/tags/'+source['tag']+':refs/tags/'+source['tag']])
        revision = checked(['git', '-C', repo, 'rev-parse', source['tag']+'^{}']).strip()
        if revision != source['revision']:
            raise ValueError('Official tag revision mismatch')
        tag_status = checked(['git', '-C', repo, '-c', 'gpg.program=gpg', 'verify-tag', '--raw', source['tag']], env)
        valid_signature(tag_status, source['tag_key_fingerprint'])
        epoch = int(checked(['git', '-C', repo, 'show', '-s', '--format=%ct', revision]).strip())
        if epoch != spec['source_date_epoch']:
            raise ValueError('SOURCE_DATE_EPOCH differs from the signed source commit')
        tar_tree = tree(archive.read_bytes(), 'ffmpeg-'+source['version']+'/')
        git_tree = tree(subprocess.check_output(['git', '-C', str(repo), 'archive', revision]))
        # Upstream documents only .git* removal and generated VERSION as differences.
        removed = sorted(name for name in git_tree if PurePosixPath(name).name.startswith('.git'))
        for name in removed:
            del git_tree[name]
        expected_version = hashlib.sha256((source['version']+'\n').encode()).hexdigest()
        if tar_tree.pop('VERSION', None) != {'sha256': expected_version, 'executable': False}:
            raise ValueError('Unexpected generated VERSION')
        if tar_tree != git_tree:
            differences = sorted(k for k in tar_tree.keys() | git_tree.keys() if tar_tree.get(k) != git_tree.get(k))
            raise ValueError('Archive/tag differences: '+repr(differences[:40]))
        report = {'schema': 1, 'status': 'passed', 'archive_sha256': digest(archive),
                  'source_revision': revision, 'source_date_epoch': epoch,
                  'release_key_fingerprint': source['release_key_fingerprint'],
                  'tag_key_fingerprint': source['tag_key_fingerprint'],
                  'release_signature': status, 'tag_signature': tag_status,
                  'compared_files': len(git_tree), 'excluded_git_files': removed,
                  'tree_sha256': hashlib.sha256(json.dumps(git_tree, sort_keys=True).encode()).hexdigest()}
        for name in ['release-key.asc', 'tag-key.asc', 'release.asc']:
            (cache/('ffmpeg-'+name)).write_bytes((work/name).read_bytes())
        return report


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--cache', type=Path, required=True)
    p.add_argument('--report', type=Path, required=True)
    a = p.parse_args()
    report = verify(a.cache.resolve())
    a.report.parent.mkdir(parents=True, exist_ok=True)
    a.report.write_text(json.dumps(report, indent=2)+'\n')
    print('Official source provenance passed:', a.report)
