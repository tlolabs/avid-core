import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from build import SPEC_PATH, digest
from hardware_probe import probe
from package import validate_qualification


class QualificationTests(unittest.TestCase):
    def test_non_macos_never_probes_hardware(self):
        for target in ['windows-x86_64', 'windows-arm64', 'linux-x86_64', 'linux-arm64']:
            with patch('hardware_probe.capture') as capture, patch('hardware_probe.subprocess.run') as run:
                self.assertEqual(probe(Path('/missing'), target)['status'], 'not_required')
                capture.assert_not_called()
                run.assert_not_called()

    def test_unavailable_optional_macos_does_not_raise(self):
        self.assertEqual(probe(Path('/missing'), 'macos-arm64')['status'], 'unavailable')

    def test_all_targets_require_software_and_only_macos_enables_optional_videotoolbox(self):
        spec = json.loads(SPEC_PATH.read_text())
        self.assertTrue({'libx264', 'libx265', 'aac'} <= set(spec['required']['encoders']))
        for t in spec['targets']:
            self.assertFalse(set(t['dependencies']) - {'x264', 'x265', 'lame', 'zlib', 'nasm'})
            self.assertFalse(any(e.startswith(('h264_', 'hevc_')) for e in t['required_encoders']))
            self.assertEqual(t['optional_encoders'], ['h264_videotoolbox', 'hevc_videotoolbox'] if t['os'] == 'macos' else [])

    def test_hardware_is_optional_but_required_evidence_cannot_be_waived(self):
        target = 'windows-x86_64'
        hashes = {'ffmpeg.exe': 'a'*64, 'ffprobe.exe': 'b'*64}
        with tempfile.TemporaryDirectory() as d:
            root = Path(d).resolve()
            docs = root/'docs/ffmpeg'
            docs.mkdir(parents=True)
            gates = {}
            for gate in ['software_encoding', 'minimum_os', 'toolchain', 'host_packaging']:
                report = docs/(gate+'.json')
                report.write_text(json.dumps({'status': 'passed', 'gate': gate, 'target': target, 'binary_sha256': hashes}))
                gates[gate] = {'status': 'passed', 'evidence': [{'path': str(report.relative_to(root)), 'sha256': digest(report)}]}
            q = {'targets': {target: gates}}
            with patch('package.ROOT', root):
                for state in ['not_required', 'deferred', 'unavailable', 'failed', 'not_run']:
                    gates['hardware_encoding'] = {'status': state, 'evidence': []}
                    validate_qualification(q, target, hashes)
                for gate in ['software_encoding', 'minimum_os', 'toolchain', 'host_packaging']:
                    bad = copy.deepcopy(q)
                    bad['targets'][target][gate]['status'] = 'not_required'
                    with self.assertRaisesRegex(ValueError, gate):
                        validate_qualification(bad, target, hashes)
                with self.assertRaisesRegex(ValueError, 'exact executable pair'):
                    validate_qualification(q, target, {'ffmpeg.exe': 'c'*64})
