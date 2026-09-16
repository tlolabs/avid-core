import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from build import SPEC_PATH, artifact_name, digest
from validate import names
from package import package, promote


class ContractTests(unittest.TestCase):
    def test_exact_capability_names_and_format_aliases(self):
        found = names(' V..... other mentions libx264\n DE mov,mp4 Movie\n  file\n D  lavfi Device\n')
        self.assertNotIn('libx264', found)
        self.assertTrue({'mov', 'mp4', 'file', 'lavfi'} <= found)

    def test_matrix_preserves_all_distributed_targets(self):
        s = json.loads(SPEC_PATH.read_text())
        self.assertEqual({t['id'] for t in s['targets']}, {f'{o}-{a}' for o in ['macos','windows','linux'] for a in ['arm64','x86_64']})
        for dep in [s['source'], *s['dependencies'].values()]:
            self.assertTrue('sha256' in dep or len(dep.get('revision','')) == 40)
        self.assertNotIn('--enable-nonfree', s['configure'])
        self.assertNotIn('--enable-version3', s['configure'])

    def test_unqualified_release_is_rejected_even_with_files(self):
        with tempfile.TemporaryDirectory() as d:
            spec=Path(d)/'candidate.json';s=json.loads(SPEC_PATH.read_text());s['status']='candidate';spec.write_text(json.dumps(s))
            with patch('package.SPEC_PATH',spec), self.assertRaisesRegex(ValueError, 'Publication blocked'):
                promote(Path(d))

    def test_qualified_release_still_requires_all_targets_and_sources(self):
        s = json.loads(SPEC_PATH.read_text())
        s.update(status='qualified', qualification_blockers=[])
        with tempfile.TemporaryDirectory() as d:
            root=Path(d); spec=root/'spec.json';spec.write_text(json.dumps(s))
            with patch('package.SPEC_PATH',spec):
                with self.assertRaisesRegex(ValueError,'Incomplete matrix'):
                    promote(root)
                for t in s['targets']:
                    for suffix in ['.tar.gz','-sources.tar.gz']:
                        p=root/(artifact_name(s,t['id'])+suffix);p.write_bytes(b'fixture')
                        p.with_name(p.name+'.sha256').write_text(digest(p)+'  '+p.name+'\n')
                # Filenames and matching checksums alone never qualify arbitrary bytes.
                with self.assertRaises(Exception):
                    promote(root)
                p.write_bytes(b'changed')
                with self.assertRaises(Exception):
                    promote(root)

    def test_packaging_rejects_baseline_validation(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            (root/'build.json').write_text('{"target":"macos-arm64"}')
            (root/'validation.json').write_text('{"baseline":true,"target":"macos-arm64"}')
            with self.assertRaisesRegex(ValueError, 'non-baseline'):
                package(root,root/'out')

if __name__ == '__main__':
    unittest.main()
