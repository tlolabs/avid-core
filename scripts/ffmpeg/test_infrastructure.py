"""Diagnostic research tests; no Core release or host packaging gate."""
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from build import SPEC_PATH, artifact_name, digest
from validate import names


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

if __name__ == '__main__':
    unittest.main()
