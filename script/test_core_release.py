import copy
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
import zipfile
from unittest.mock import patch
import core_runtime as acquire
import core_release as release

ROOT=Path(__file__).resolve().parents[1]

class ReleaseControls(unittest.TestCase):
    def test_candidate_cannot_be_production_fallback(self):
        with self.assertRaisesRegex(ValueError,'qualification'):
            acquire.candidate({},'macos-arm64',Path('missing'),False)

    def test_release_requires_exact_authenticated_pin(self):
        with self.assertRaises((ValueError,KeyError)):
            acquire.release({'repository':acquire.REPOSITORY,'release_tag':'latest'},'macos-arm64',Path('missing'))

    def test_duplicate_manifest_keys_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/'manifest.json'
            path.write_text('{"target":"macos-arm64","target":"windows-arm64"}')
            with self.assertRaisesRegex(ValueError,'Duplicate'):
                acquire.obj(path)

    def test_release_authentication_rejects_digest_and_signature_before_execution(self):
        with tempfile.TemporaryDirectory() as temporary:
            path=Path(temporary)/'manifest.json';path.write_text('{}')
            pin={'manifest_sha256':'0'*64}
            with patch('core_runtime.subprocess.run') as process:
                with self.assertRaisesRegex(ValueError,'Manifest'):
                    acquire.authenticate_manifest(pin,path)
                process.assert_not_called()
            pin.update(manifest_sha256=acquire.digest(path),release_revision='a'*40,release_tag='ffmpeg-9.0.1-r7.1')
            import subprocess
            with patch('core_runtime.subprocess.run',side_effect=subprocess.CalledProcessError(1,['gh','attestation'])):
                with self.assertRaises(subprocess.CalledProcessError):
                    acquire.authenticate_manifest(pin,path)

    def test_original_assets_remain_required_for_cached_runtime(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);runtime=root/'runtime';runtime.mkdir()
            (root/'runtime.tar.gz').write_bytes(b'tampered')
            pin={'targets':{'macos-arm64':{'archive':'runtime.tar.gz','sha256':'0'*64}}}
            with self.assertRaisesRegex(ValueError,'Cached original'):
                acquire.verify_receipt(pin,'macos-arm64',runtime,True)

    def test_hosted_os_approval_and_gate_cannot_drift(self):
        spec=acquire.obj(ROOT/'runtime/ffmpeg/spec.json');ledger=acquire.obj(ROOT/'runtime/ffmpeg/qualification.json')
        release.validate_policy(spec,ledger)
        broken=copy.deepcopy(ledger);broken['targets']['macos-arm64']['older_os']['release_gate']=True
        with self.assertRaisesRegex(ValueError,'Superseded'):
            release.validate_policy(spec,broken)
        broken=copy.deepcopy(ledger);broken['os_policy']['approval_record']='different'
        with self.assertRaisesRegex(ValueError,'drifted'):
            release.validate_policy(spec,broken)

    def test_complete_matrix_required(self):
        spec=acquire.obj(ROOT/'runtime/ffmpeg/spec.json');ledger=acquire.obj(ROOT/'runtime/ffmpeg/qualification.json')
        ledger['targets'].pop('windows-arm64')
        with self.assertRaisesRegex(ValueError,'Complete'):
            release.validate_policy(spec,ledger)

    def test_archive_traversal_links_duplicates_rejected(self):
        for name in ['../escape','/absolute','drive:file','x/../bad','x\\bad','x//bad']:
            with self.assertRaises(ValueError):acquire.safe_name(name)
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);archive=root/'bad.tar.gz'
            with tarfile.open(archive,'w:gz') as tar:
                member=tarfile.TarInfo('runtime/link');member.type=tarfile.SYMTYPE;member.linkname='../outside';tar.addfile(member)
            with self.assertRaises(ValueError):acquire.extract_runtime(archive,root/'out','runtime')
            z=root/'bad.zip'
            with zipfile.ZipFile(z,'w') as stream:stream.writestr('../outside','bad')
            with self.assertRaises(ValueError):acquire.extract_zip(z,root/'out')

    def test_pr_run_and_unperformed_steps_cannot_be_promoted(self):
        plan=acquire.obj(ROOT/'runtime/releases/ffmpeg-9.0.1-r7.1.plan.json')
        run={'id':plan['build_run'],'run_attempt':plan['build_attempt'],'head_sha':plan['build_revision'],
             'head_branch':plan['build_branch'],'path':plan['build_workflow'],'event':'pull_request',
             'repository':{'full_name':acquire.REPOSITORY},'head_repository':{'full_name':acquire.REPOSITORY},
             'status':'completed','conclusion':'success'}
        with self.assertRaisesRegex(ValueError,'origin'):acquire.check_origin(plan,run,[],[])
        run['event']='workflow_dispatch'
        with self.assertRaisesRegex(ValueError,'Missing artifact'):acquire.check_origin(plan,run,[],[])

    def test_failed_sibling_job_does_not_discard_passing_native_origin(self):
        from import_host_evidence import validate_origin
        run={'id':1,'repository':{'full_name':'tlolabs/ativ'},'head_repository':{'full_name':'tlolabs/ativ'},
             'head_sha':'a'*40,'event':'workflow_dispatch','path':'.github/workflows/native-release.yml',
             'status':'completed','conclusion':'failure'}
        validate_origin(run,'tlolabs/ativ','a'*40,1)
        run['event']='pull_request'
        with self.assertRaisesRegex(ValueError,'origin'):validate_origin(run,'tlolabs/ativ','a'*40,1)
        run['event']='workflow_dispatch';run['conclusion']='cancelled'
        with self.assertRaisesRegex(ValueError,'origin'):validate_origin(run,'tlolabs/ativ','a'*40,1)

    def test_inaccurate_optional_host_report_is_rejected(self):
        with self.assertRaises((ValueError,KeyError)):
            release.validate_host({'schema':1,'target':'macos-arm64','status':'passed'},'macos-arm64',{}, {})


class OwnerIntegrationPolicy(unittest.TestCase):
    def test_owner_acceptance_removes_application_prerequisites(self):
        spec=acquire.obj(ROOT/'runtime/ffmpeg/spec.json');ledger=acquire.obj(ROOT/'runtime/ffmpeg/qualification.json')
        for entry in ledger['targets'].values():
            entry['host_packaging']['evidence']=[]
            entry['host_packaging']['status']='not_run'
        release.validate_policy(spec,ledger)
        ledger.pop('release_acceptance')
        with self.assertRaisesRegex(ValueError,'Owner'):release.validate_policy(spec,ledger)

if __name__=='__main__':unittest.main()
