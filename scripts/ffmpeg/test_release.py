import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from build import SPEC_PATH, artifact_name, digest
from release_manifest import verify_manifest
from qualify_archive import TESTS, verify_receipt


class ReleaseEvidenceTests(unittest.TestCase):
    def test_manifest_rejects_missing_targets_and_changed_identity(self):
        spec=json.loads(SPEC_PATH.read_text());revision='a'*40
        m={'schema':1,'status':'qualified','core_revision':revision,'spec_sha256':digest(SPEC_PATH),
           'recipe':spec['recipe'],'source':spec['source'],
           'tag':f'ffmpeg-{spec["source"]["version"]}-r{spec["recipe"]}', 'targets':{}}
        for t in spec['targets']:
            name=artifact_name(spec,t['id'])
            m['targets'][t['id']]={'status':'passed','qualification_os':t['qualification_os'],
                'native_host':{'machine':t['arch']},'runtime':{'asset':name+'.tar.gz','sha256':'b'*64},
                'sources':{'asset':name+'-sources.tar.gz','sha256':'c'*64}}
        verify_manifest(m,spec,revision)
        for key,value in [('status','candidate'),('core_revision','d'*40),('spec_sha256','e'*64),('recipe',0)]:
            bad=copy.deepcopy(m);bad[key]=value
            with self.subTest(key=key), self.assertRaises(ValueError):verify_manifest(bad,spec,revision)
        bad=copy.deepcopy(m);bad['targets'].pop('windows-x86_64')
        with self.assertRaises(ValueError):verify_manifest(bad,spec,revision)
        bad=copy.deepcopy(m);bad['targets']['windows-x86_64']['runtime']=m['targets']['windows-arm64']['runtime']
        with self.assertRaises(ValueError):verify_manifest(bad,spec,revision)

    def test_archive_receipt_binds_archive_pair_identity_and_actual_test_log(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);archive=root/'runtime.tar.gz';archive.write_bytes(b'qualified archive')
            log=root/(archive.name+'.installation.log');log.write_text('all required native tests passed')
            build={'target':'windows-x86_64','core_revision':'a'*40,'spec_sha256':'b'*64,
                   'native_host':{'system':'Windows','machine':'AMD64'}}
            hashes={'ffmpeg.exe':'c'*64,'ffprobe.exe':'d'*64}
            files={'build.json':json.dumps(build).encode(),'validation.json':json.dumps({'binary_sha256':hashes}).encode(),
                   'SOURCE.json':json.dumps({'sha256':'e'*64}).encode()}
            r=dict(build,status='passed',runtime_sha256=digest(archive),source_sha256='e'*64,
                   binary_sha256=hashes,tests=TESTS,log=log.name,log_sha256=digest(log))
            receipt=root/(archive.name+'.qualification.json');receipt.write_text(json.dumps(r))
            verify_receipt(root,archive,files)
            for key,value in [('status','failed'),('runtime_sha256','f'*64),('target','windows-arm64'),('tests',TESTS[:-1]),('log','../outside')]:
                bad=copy.deepcopy(r);bad[key]=value;receipt.write_text(json.dumps(bad))
                with self.subTest(key=key),self.assertRaises(ValueError):verify_receipt(root,archive,files)
            receipt.write_text(json.dumps(r));log.write_text('replacement log')
            with self.assertRaises(ValueError):verify_receipt(root,archive,files)
            log.write_text('all required native tests passed');archive.write_bytes(b'rebuilt archive')
            with self.assertRaises(ValueError):verify_receipt(root,archive,files)
