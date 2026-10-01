#!/usr/bin/env python3
# SPDX-FileCopyrightText: Thomas Lothian
# SPDX-License-Identifier: GPL-3.0-or-later
"""Import an exact native host report from a pinned, successful non-PR run.

This authenticates packaging evidence; it cannot turn unrun signing, upgrades or
manual acceptance into passes. The release gate independently requires them.
"""
import argparse
import json
from pathlib import Path
import re
import subprocess
import tempfile
from core_runtime import TARGETS, digest, extract_zip, gh_json, obj, require

WORKFLOWS = {'tlolabs/ativ': '.github/workflows/native-release.yml',
             'tlolabs/encap': '.github/workflows/build-platforms.yml'}


def import_report(repo, revision, run_id, target, output):
    require(repo in WORKFLOWS and target in TARGETS and re.fullmatch('[0-9a-f]{40}', revision), 'Exact approved host identity required')
    run = gh_json(f'repos/{repo}/actions/runs/{run_id}')
    require(run['id'] == run_id and run['repository']['full_name'] == repo and
            run['head_repository']['full_name'] == repo and run['head_sha'] == revision and
            run['event'] == 'workflow_dispatch' and run['path'] == WORKFLOWS[repo] and
            run['status'] == 'completed' and run['conclusion'] == 'success', 'Untrusted host workflow origin')
    artifacts = gh_json(f'repos/{repo}/actions/runs/{run_id}/artifacts?per_page=100')['artifacts']
    system, arch = target.split('-', 1)
    labels = {'macos': 'macOS', 'windows': 'Windows', 'linux': 'Linux'}
    label = arch
    if repo.endswith('/encap'):
        label = {'macos-x86_64': 'intel', 'windows-x86_64': 'x64', 'linux-x86_64': 'x64'}.get(target, arch)
        artifact_name = f'EnCap-{system}-{label}'
    else:
        label = {'windows-arm64': 'ARM64', 'windows-x86_64': 'x64', 'linux-arm64': 'aarch64'}.get(target, arch)
        artifact_name = f'ATIV-{system}-{label}'
    matches = [a for a in artifacts if a['name'] == artifact_name and not a['expired']]
    require(len(matches) == 1 and re.fullmatch(r'sha256:[0-9a-f]{64}', matches[0].get('digest', '')), 'Missing authenticated host artifact')
    artifact = matches[0]
    require(artifact['workflow_run']['id'] == run_id and artifact['workflow_run']['head_sha'] == revision, 'Host artifact revision mismatch')
    jobs = gh_json(f'repos/{repo}/actions/runs/{run_id}/attempts/{run["run_attempt"]}/jobs?per_page=100')['jobs']
    native = [j for j in jobs if j['name'] == labels[system]+' '+label and j['conclusion'] == 'success']
    require(len(native) == 1, 'Native host job did not pass')
    names = {s['name'] for s in native[0]['steps'] if s['conclusion'] == 'success'}
    require(any('host evidence' in name.lower() for name in names) and
            any('package' in name.lower() for name in names), 'Required package/evidence steps did not pass')
    require(not output.exists(), 'Host evidence output must be new')
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        archive = root / 'artifact.zip'
        with archive.open('wb') as stream:
            subprocess.run(['gh', 'api', f'repos/{repo}/actions/artifacts/{artifact["id"]}/zip'], stdout=stream, check=True)
        require(digest(archive) == artifact['digest'].removeprefix('sha256:'), 'Host artifact digest mismatch')
        extract_zip(archive, root / 'payload')
        reports = list((root / 'payload').rglob('host-'+target+'.json'))
        require(len(reports) == 1, 'Missing exact target host report')
        report = obj(reports[0])
        require(report['target'] == target and report['native_target'] == target and
                report['application_repository'] == repo and report['application_revision'] == revision and
                str(report['evidence_origin']['run_id']) == str(run_id) and
                report['evidence_origin']['event'] == 'workflow_dispatch', 'Host report origin/target mismatch')
        for name in ('packaging', 'launch', 'media', 'lifecycle'):
            require(report['checks'][name]['status'] == 'passed', 'Unperformed native host check: '+name)
        for package in report['packages']:
            paths = list((root / 'payload').rglob(package['filename']))
            require(len(paths) == 1 and digest(paths[0]) == package['sha256'] and
                    paths[0].stat().st_size == package['size'], 'Host package bytes differ from evidence')
        report['evidence_origin'].update(artifact_digest_verified=True, artifact_id=artifact['id'],
                                       artifact_zip_sha256=digest(archive), run_attempt=run['run_attempt'],
                                       workflow=WORKFLOWS[repo], native_job_id=native[0]['id'])
        report['authenticated_origin'] = {'run': run, 'artifact': artifact, 'native_job': native[0]}
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(report, indent=2)+'\n')
    print(str(output)+': authenticated native packaging evidence; no unrun check was changed')


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--repository', required=True, choices=sorted(WORKFLOWS))
    p.add_argument('--revision', required=True)
    p.add_argument('--run', required=True, type=int)
    p.add_argument('--target', required=True, choices=sorted(TARGETS))
    p.add_argument('--output', required=True, type=Path)
    a = p.parse_args()
    import_report(a.repository, a.revision, a.run, a.target, a.output)
