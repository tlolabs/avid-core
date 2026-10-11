#!/usr/bin/env python3
"""Publish one validated native candidate as an immutable direct-download release.

This is deliberately separate from the six-target authenticated runtime manifest.
The authenticated updater continues to use runtime-release.yml and its existing tag.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time
from urllib.error import HTTPError
from urllib.parse import quote
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts/ffmpeg'))
from artifact import validate_runtime, validate_sources  # noqa: E402
from build import SPEC_PATH, artifact_name, digest  # noqa: E402
from core_runtime import extract_zip, require  # noqa: E402

REPOSITORY = 'tlolabs/avid-core'
BUILD_WORKFLOWS = {
    '.github/workflows/ffmpeg.yml',  # Existing verified candidates.
    '.github/workflows/ffmpeg-independent.yml',
}
BUILD_STEP = {
    'windows': 'Build and validate native Windows runtime',
    'unix': 'Build and validate native Unix runtime',
}
UPLOAD_STEP = 'Upload validated Core runtime candidate and corresponding source'
PUBLICATION_NOTE = (
    'The release tag identifies the publication revision on main; the embedded '
    'platform-release.json and source archive identify the exact validated build.\n\n'
)


def gh(*args, output=None, check=True):
    require(bool(os.environ.get('GH_TOKEN')), 'GH_TOKEN is required for publication')
    if output:
        with output.open('wb') as stream:
            result = subprocess.run(['gh', *args], stdout=stream, stderr=subprocess.PIPE)
    else:
        result = subprocess.run(['gh', *args], stdout=subprocess.DEVNULL,
                                stderr=subprocess.PIPE)
    if check and result.returncode:
        raise RuntimeError(result.stderr.decode(errors='replace').strip() or 'GitHub CLI failed')
    return result


def gh_json(path, missing_ok=False):
    result = subprocess.run(['gh', 'api', path], capture_output=True, text=True)
    if result.returncode:
        if missing_ok and 'HTTP 404' in result.stderr:
            return None
        raise RuntimeError(result.stderr.strip() or 'GitHub API request failed')
    return json.loads(result.stdout)


def wait_for_json(path, ready, attempts=5, missing_ok=False):
    for attempt in range(attempts):
        value = gh_json(path, missing_ok=missing_ok)
        if ready(value):
            return value
        if attempt + 1 < attempts:
            time.sleep(2)
    return value


def release_api(method, path, payload=None, upload=False, missing_ok=False):
    token = os.environ.get('GH_TOKEN')
    require(bool(token), 'GH_TOKEN is required for publication')
    base = 'https://uploads.github.com' if upload else 'https://api.github.com'
    data = None if payload is None else (
        payload if isinstance(payload, bytes) else json.dumps(payload).encode()
    )
    headers = {
        'Authorization': 'Bearer ' + token,
        'User-Agent': 'avid-core-platform-publisher',
        'X-GitHub-Api-Version': '2022-11-28',
        'Accept': 'application/vnd.github+json',
    }
    if data is not None:
        headers['Content-Type'] = 'application/octet-stream' if upload else 'application/json'
    request = Request(base + '/' + path.lstrip('/'), data=data, method=method,
                      headers=headers)
    try:
        with urlopen(request, timeout=120) as response:
            return json.load(response)
    except HTTPError as error:
        if missing_ok and error.code == 404:
            return None
        detail = error.read(1000).decode(errors='replace')
        raise RuntimeError(f'GitHub release API HTTP {error.code}: {detail}') from error


def find_release(tag):
    matches = []
    for page in range(1, 11):
        releases = release_api('GET', f'repos/{REPOSITORY}/releases?per_page=100&page={page}')
        matches.extend(release for release in releases if release['tag_name'] == tag)
        if len(releases) < 100:
            break
    require(len(matches) <= 1, 'Duplicate draft/published release identity')
    return matches[0] if matches else None


def get_release(release_id):
    return release_api('GET', f'repos/{REPOSITORY}/releases/{release_id}')


def validated_origin(run, jobs, artifacts, target, spec):
    targets = {item['id']: item for item in spec['targets']}
    require(target in targets, 'Unknown platform target')
    require(run['repository']['full_name'] == REPOSITORY and
            run['head_repository']['full_name'] == REPOSITORY and
            run['head_branch'] == 'main' and run['event'] in ('push', 'workflow_dispatch') and
            run['path'] in BUILD_WORKFLOWS and
            re.fullmatch(r'[0-9a-f]{40}', run['head_sha']) is not None,
            'Candidate did not originate from the AVID Core main workflow')
    matching_jobs = [job for job in jobs
                     if job['name'].split(' / ')[-1] == f'build ({target})']
    require(len(matching_jobs) == 1, 'Expected exactly one target build job')
    job = matching_jobs[0]
    passed = {step['name'] for step in job.get('steps', []) if step.get('conclusion') == 'success'}
    required = {BUILD_STEP['windows' if target.startswith('windows-') else 'unix'], UPLOAD_STEP}
    require(job['conclusion'] == 'success' and required <= passed,
            'Target native tests or candidate upload did not pass')
    matching_artifacts = [artifact for artifact in artifacts
                          if artifact['name'] == f'core-runtime-candidate-{target}']
    require(len(matching_artifacts) == 1, 'Expected exactly one target candidate artifact')
    artifact = matching_artifacts[0]
    require(not artifact['expired'] and
            re.fullmatch(r'sha256:[0-9a-f]{64}', artifact.get('digest') or '') is not None and
            artifact['workflow_run']['id'] == run['id'] and
            artifact['workflow_run']['head_sha'] == run['head_sha'],
            'Candidate artifact identity is incomplete or mismatched')
    return job, artifact


def get_origin(run_id, target, spec):
    run = gh_json(f'repos/{REPOSITORY}/actions/runs/{run_id}')
    require(run['id'] == run_id, 'Unexpected candidate run ID')
    jobs = gh_json(f'repos/{REPOSITORY}/actions/runs/{run_id}/jobs?per_page=100')['jobs']
    artifact_response = wait_for_json(
        f'repos/{REPOSITORY}/actions/runs/{run_id}/artifacts?per_page=100',
        lambda data: any(a['name'] == f'core-runtime-candidate-{target}'
                         for a in data['artifacts']),
    )
    artifacts = artifact_response['artifacts']
    job, artifact = validated_origin(run, jobs, artifacts, target, spec)
    return run, job, artifact


def prepare_assets(directory, run, job, artifact, target, spec):
    zip_path = directory / 'candidate.zip'
    gh('api', f'repos/{REPOSITORY}/actions/artifacts/{artifact["id"]}/zip', output=zip_path)
    require(digest(zip_path) == artifact['digest'].removeprefix('sha256:'),
            'Downloaded Actions archive digest mismatch')
    payload = directory / 'payload'
    payload.mkdir()
    extract_zip(zip_path, payload)
    base = artifact_name(spec, target)
    runtime = payload / 'packages' / (base + '.tar.gz')
    sources = payload / (base + '-sources.tar.gz')
    checksum = payload / 'packages' / (base + '.tar.gz.sha256')
    require({p.relative_to(payload).as_posix() for p in payload.rglob('*') if p.is_file()} ==
            {f'packages/{base}.tar.gz', f'packages/{base}.tar.gz.sha256',
             f'{base}-sources.tar.gz'}, 'Candidate archive has unexpected files')
    require(checksum.read_text() == f'{digest(runtime)}  {runtime.name}\n',
            'Candidate package checksum file mismatch')
    validate_runtime(runtime, spec, target, core_revision=run['head_sha'], clean=True)
    validate_sources(sources, spec, target)
    output = directory / 'release-assets'
    output.mkdir()
    for source in (runtime, sources, checksum):
        (output / source.name).write_bytes(source.read_bytes())
    provenance = {
        'schema': 1,
        'kind': 'direct-download-platform',
        'repository': REPOSITORY,
        'target': target,
        'build_run': run['id'],
        'build_attempt': run['run_attempt'],
        'build_revision': run['head_sha'],
        'native_job_id': job['id'],
        'artifact_id': artifact['id'],
        'artifact_zip_sha256': artifact['digest'].removeprefix('sha256:'),
        'spec_sha256': hashlib.sha256(SPEC_PATH.read_bytes()).hexdigest(),
        'runtime_archive': runtime.name,
        'runtime_sha256': digest(runtime),
        'source_archive': sources.name,
        'source_sha256': digest(sources),
    }
    (output / 'platform-release.json').write_text(json.dumps(provenance, indent=2, sort_keys=True) + '\n')
    checksums = ''.join(f'{digest(path)}  {path.name}\n' for path in sorted(output.iterdir()))
    (output / 'SHA256SUMS').write_text(checksums)
    return output, provenance


def release_identity(run, target, spec, provenance):
    tag = f'platform-ffmpeg-{spec["source"]["version"]}-r{spec["recipe"]}-{target}-run{run["id"]}'
    title = f'FFmpeg {spec["source"]["version"]} r{spec["recipe"]} · {target} · run {run["id"]}'
    prefix = (
        f'Verified direct-download FFmpeg/FFprobe pair for {target}.\n\n'
        f'Build: https://github.com/{REPOSITORY}/actions/runs/{run["id"]}\n'
        f'Build revision: {run["head_sha"]}\n'
        f'Specification SHA-256: {provenance["spec_sha256"]}\n\n'
    )
    notes = prefix + PUBLICATION_NOTE + (
        'The runtime archive, corresponding sources, SHA256SUMS, and platform-release.json '
        'are verified together before publication. This platform release contains no '
        'authenticated six-target runtime manifest. Existing applications remain pinned '
        'to ffmpeg-9.0.1-r7.1 and its signed update verification.\n'
    )
    return tag, title, notes


def release_metadata(release, tag, title, notes, revision):
    matches = {
        'tag': release['tag_name'] == tag,
        'name': release['name'] == title,
        'body': release['body'].rstrip('\n') == notes.rstrip('\n'),
        'target': release['target_commitish'] == 'main',
        'prerelease': release['prerelease'] is False,
    }
    require(all(matches.values()),
            'Existing release metadata conflicts with validated build: ' +
            ', '.join(key for key, matched in matches.items() if not matched))
    if not release['draft']:
        ref = release_api('GET', f'repos/{REPOSITORY}/git/ref/tags/{tag}')
        require(ref['object']['type'] == 'commit' and
                re.fullmatch(r'[0-9a-f]{40}', ref['object']['sha']) is not None,
                'Platform release tag is not a commit')
        ancestry = release_api('GET', f'repos/{REPOSITORY}/compare/{revision}...{ref["object"]["sha"]}')
        require(ancestry['status'] in ('identical', 'ahead'),
                'Platform publication revision does not descend from its validated build')


def create_or_get_release(tag, title, notes, revision):
    release = find_release(tag)
    if release is None:
        try:
            release = release_api('POST', f'repos/{REPOSITORY}/releases', {
                'tag_name': tag,
                'target_commitish': 'main',
                'name': title,
                'body': notes,
                'draft': True,
                'prerelease': False,
                'make_latest': 'false',
            })
        except RuntimeError:
            release = find_release(tag)
            if release is None:
                raise
    if (release['draft'] and release['tag_name'] == tag and release['name'] == title and
            release['target_commitish'] in (revision, 'main') and
            release['prerelease'] is False and not release['assets'] and
            f'Build revision: {revision}' in release['body'] and
            (release['target_commitish'] != 'main' or
             release['body'].rstrip('\n') != notes.rstrip('\n'))):
        # A previous publisher created this exact empty draft before GitHub rejected
        # its tag target. Recover it without creating a second release or asset.
        release = release_api('PATCH', f'repos/{REPOSITORY}/releases/{release["id"]}', {
            'target_commitish': 'main',
            'body': notes,
            'make_latest': 'false',
        })
    release_metadata(release, tag, title, notes, revision)
    return release


def verify_asset(asset, expected):
    require(asset['state'] == 'uploaded' and asset['size'] == expected.stat().st_size and
            asset['digest'] == 'sha256:' + digest(expected),
            'GitHub asset digest or size does not match validated bytes: ' + asset['name'])


def publish_assets(release, tag, output):
    expected = {path.name: path for path in output.iterdir()}
    provenance = json.loads((output / 'platform-release.json').read_text())
    require(set(expected) == {
        provenance['runtime_archive'], provenance['runtime_archive'] + '.sha256',
        provenance['source_archive'], 'platform-release.json', 'SHA256SUMS',
    }, 'Unexpected platform release assets')
    for name, path in expected.items():
        release = get_release(release['id'])
        assets = [asset for asset in release['assets'] if asset['name'] == name]
        require(len(assets) <= 1, 'Duplicate release asset name')
        if not assets:
            require(release['draft'], 'Published immutable release is missing an expected asset')
            try:
                uploaded = release_api(
                    'POST',
                    f'repos/{REPOSITORY}/releases/{release["id"]}/assets?name={quote(name, safe="")}',
                    path.read_bytes(), upload=True,
                )
            except RuntimeError:
                release = get_release(release['id'])
                assets = [asset for asset in release['assets'] if asset['name'] == name]
                if not assets:
                    raise
            else:
                assets = [uploaded]
        verify_asset(assets[0], path)
    release = get_release(release['id'])
    require({asset['name'] for asset in release['assets']} == set(expected),
            'Release has missing or unexpected assets')
    if release['draft']:
        release = release_api('PATCH', f'repos/{REPOSITORY}/releases/{release["id"]}',
                              {'draft': False, 'make_latest': 'false'})
    require(not release['draft'], 'Platform release is still a draft')
    require({asset['name'] for asset in release['assets']} == set(expected),
            'Published release assets changed during publication')
    for name, path in expected.items():
        verify_asset(next(asset for asset in release['assets'] if asset['name'] == name), path)
    return release


def publish(run_id, target):
    spec = json.loads(SPEC_PATH.read_text())
    run, job, artifact = get_origin(run_id, target, spec)
    with tempfile.TemporaryDirectory(prefix='avid-platform-release-') as temp:
        directory = Path(temp)
        output, provenance = prepare_assets(directory, run, job, artifact, target, spec)
        tag, title, notes = release_identity(run, target, spec, provenance)
        release = create_or_get_release(tag, title, notes, run['head_sha'])
        release = publish_assets(release, tag, output)
        release_metadata(release, tag, title, notes, run['head_sha'])
        print('Verified platform release:', release['html_url'])


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run-id', required=True, type=int)
    parser.add_argument('--target', required=True)
    args = parser.parse_args()
    publish(args.run_id, args.target)
