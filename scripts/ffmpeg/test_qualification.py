"""Diagnostic research tests; no Core release or host packaging gate."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from build import SPEC_PATH, digest
from hardware_probe import probe


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
