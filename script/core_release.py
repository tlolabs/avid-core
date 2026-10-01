#!/usr/bin/env python3
# SPDX-FileCopyrightText: Thomas Lothian
# SPDX-License-Identifier: GPL-3.0-or-later
"""Promote exact authenticated candidates; never relabel promotion as a build."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
from core_runtime import TARGETS, check_origin, digest, obj, pair, require, verify_directory

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'scripts/ffmpeg'))
from artifact import validate_runtime, validate_sources


def validate_policy(spec, ledger):
    require(ledger['schema'] == 3 and set(ledger['targets']) == TARGETS, 'Complete qualification ledger required')
    require(spec['qualification_policy']['host_packaging'] == 'required', 'Host prerequisite must remain required')
    approved = spec['qualification_policy']['older_os']
    require('hosted runner OS qualification authorized by user on 2026-09-16' in approved,
            'Hosted-runner approval missing from specification')
    require(ledger['os_policy']['approval_record'] == approved and ledger['os_policy']['basis'] == 'actual_native_host',
            'OS approval and executable gate have drifted')
    for target, entry in ledger['targets'].items():
        require(entry['older_os']['status'] == 'untested' and entry['older_os']['release_gate'] is False,
                'Superseded OS gate cannot block publication or become a compatibility claim')
        require(entry['host_packaging']['required'] is True, 'Genuine host prerequisite removed')


def validate_host(report, target, record, plan):
    require(report['schema'] == 1 and report['target'] == target and report['status'] == 'passed' and
            report['core_build_revision'] == plan['build_revision'] and report['core_build_run'] == plan['build_run'] and
            report['runtime_archive_sha256'] == record['sha256'] and report['original_binary_sha256'] == record['binary_sha256'],
            'Host evidence does not apply to the promoted pair')
    require(report['application_repository'] in {'tlolabs/ativ', 'tlolabs/encap'} and
            len(report['application_revision']) == 40 and report['application_version'], 'Host application identity missing')
    required = {'packaging', 'launch', 'media', 'lifecycle', 'signing', 'authenticated_upgrade'}
    require(required <= report['checks'].keys() and all(report['checks'][name]['status'] == 'passed' and
            report['checks'][name]['evidence'] for name in required), 'Required host packaging/signing/upgrade evidence missing')
    require(report['native_target'] == target and report['native_environment'] and report['packages'] and
            all(len(p['sha256']) == 64 and p['filename'] for p in report['packages']), 'Native host or package evidence missing')
    require(report['evidence_origin']['event'] == 'workflow_dispatch' and
            report['evidence_origin']['repository'] == report['application_repository'] and
            report['evidence_origin']['revision'] == report['application_revision'] and
            report['evidence_origin']['artifact_digest_verified'] is True,
            'Host report must have authenticated non-PR run origin')


def inventory(plan, directory):
    spec = obj(ROOT / 'runtime/ffmpeg/spec.json')
    result = json.loads(json.dumps(plan))
    for target, record in result['targets'].items():
        root = directory / target
        origin = obj(root / 'origin.json')
        # check_origin needs the complete six-target records; one origin validates its
        # exact record here, while run/job/API collection authenticity was checked before download.
        run = origin['run']
        require(run['head_sha'] == plan['build_revision'] and run['id'] == plan['build_run'] and
                run['event'] == 'workflow_dispatch' and run['repository']['full_name'] == plan['repository'] and
                run['head_repository']['full_name'] == plan['repository'] and run['conclusion'] == 'success' and
                run['path'] == plan['build_workflow'] and run['head_branch'] == plan['build_branch'], 'Imported origin mismatch')
        require(origin['artifact']['id'] == record['artifact_id'] and origin['artifact']['digest'] == 'sha256:'+record['artifact_zip_sha256'] and
                origin['job']['id'] == record['native_job_id'] and origin['job']['conclusion'] == 'success', 'Imported artifact/job mismatch')
        archive, source = root / record['archive'], root / record['source_archive']
        require(digest(archive) == record['sha256'] and digest(source) == record['source_sha256'], 'Original asset digest mismatch')
        files = validate_runtime(archive, spec, target, plan['build_revision'], clean=True)
        validate_sources(source, spec, target)
        build = json.loads(files['build.json'])
        validation = json.loads(files['validation.json'])
        record['binary_sha256'] = validation['binary_sha256']
        record['source_revision'] = build['source_revision']
        record['recipe'] = build['recipe']
        record['version'] = build['version']
        record['spec_sha256'] = build['spec_sha256']
        record['build_sha256'] = hashlib.sha256(files['build.json']).hexdigest()
        record['build_scripts_sha256'] = build['build_scripts_sha256']
        record['configure'] = build['configure']
        record['native_host'] = build['native_host']
        record['tool_versions'] = build.get('tools', {})
        record['validation_sha256'] = hashlib.sha256(files['validation.json']).hexdigest()
        record['repeat_sha256'] = hashlib.sha256(files['repeat-build.json']).hexdigest()
        record['source_provenance_sha256'] = hashlib.sha256(files['source-provenance.json']).hexdigest()
        require(record['checksums_sha256'] == hashlib.sha256(files['SHA256SUMS']).hexdigest(), 'Internal manifest pin mismatch')
    return result


def promote(plan, directory, output, ledger, publish=False):
    spec = obj(ROOT / 'runtime/ffmpeg/spec.json')
    validate_policy(spec, ledger)
    manifest = inventory(plan, directory)
    for target, record in manifest['targets'].items():
        native = ledger['targets'][target]['native_runtime']
        require(native['status'] == 'passed' and native['build_revision'] == plan['build_revision'] and
                native['binary_sha256'] == record['binary_sha256'], 'Native ledger does not match exact pair')
        host = ledger['targets'][target]['host_packaging']
        require(host['status'] == 'passed' and host['evidence'], 'Unperformed host prerequisite: '+target)
        for evidence in host['evidence']:
            path = ROOT / evidence['path']
            require(path.resolve().is_relative_to((ROOT / 'docs/ffmpeg/evidence').resolve()) and
                    digest(path) == evidence['sha256'], 'Host evidence path/hash mismatch')
            validate_host(obj(path), target, record, plan)
    require(not output.exists(), 'Promotion output must be fresh')
    output.mkdir(parents=True)
    manifest['operation'] = 'promotion_of_existing_qualified_artifacts'
    manifest['promotion_revision'] = subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
    manifest['qualification_ledger_sha256'] = digest(ROOT / 'runtime/ffmpeg/qualification.json')
    for target, record in manifest['targets'].items():
        for name in (record['archive'], record['source_archive']):
            shutil.copy2(directory / target / name, output / name)
        record['host_evidence_assets'] = []
        for index, evidence in enumerate(ledger['targets'][target]['host_packaging']['evidence']):
            asset = output / ('host-'+target+'-'+str(index)+'.json')
            shutil.copy2(ROOT / evidence['path'], asset)
            record['host_evidence_assets'].append({'filename':asset.name,'sha256':digest(asset)})
        evidence_path=output / ('qualification-'+target+'.json')
        evidence_path.write_text(json.dumps({'native': ledger['targets'][target]['native_runtime'],
                                            'host': ledger['targets'][target]['host_packaging']},indent=2)+'\n')
        record['qualification_asset'] = evidence_path.name
        record['qualification_sha256'] = digest(evidence_path)
    (output / 'qualification.json').write_text(json.dumps(ledger,indent=2)+'\n')
    (output / 'manifest.json').write_text(json.dumps(manifest,indent=2,sort_keys=True)+'\n')
    (output / 'SHA256SUMS').write_text(''.join(digest(p)+'  '+p.name+'\n' for p in sorted(output.iterdir()) if p.is_file()))
    if publish:
        require(os.environ.get('GITHUB_REF') == 'refs/tags/'+plan['release_tag'], 'Publication must run from the exact authorized signed tag')
        existing = subprocess.run(['gh','release','view',plan['release_tag'],'--repo',plan['repository']],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        require(existing.returncode != 0, 'Release identity already exists; published bytes must never be replaced')
        subprocess.run(['gh','release','create',plan['release_tag'],'--repo',plan['repository'],'--verify-tag','--draft',
                        '--title',plan['release_tag'],'--notes','Matched source-built FFmpeg/FFprobe runtime. Original build: '+plan['build_revision']+
                        '; native qualification run: '+str(plan['build_run'])+'. This workflow promotes those exact bytes.'],check=True)
        subprocess.run(['gh','release','upload',plan['release_tag'],'--repo',plan['repository'],*[str(p) for p in sorted(output.iterdir())]],check=True)


if __name__ == '__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('mode',choices=['inventory','promote'])
    p.add_argument('--plan',type=Path,required=True);p.add_argument('--directory',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True);p.add_argument('--publish',action='store_true')
    a=p.parse_args();plan=obj(a.plan)
    if a.mode=='inventory':
        require(not a.publish, 'Inventory cannot publish')
        a.output.parent.mkdir(parents=True,exist_ok=True)
        a.output.write_text(json.dumps(inventory(plan,a.directory),indent=2)+'\n')
    else:
        promote(plan,a.directory,a.output,obj(ROOT/'runtime/ffmpeg/qualification.json'),a.publish)
