"""Offline assertions for weekly source compatibility and owner-directed scope."""
from pathlib import Path
import tempfile
import unittest

from prepare_linked_smoke import ROOT, TARGETS, release_layout


class SmokeLayoutTests(unittest.TestCase):
    def test_old_source_tags_preserve_their_native_rust_target(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.assertEqual(release_layout(root, 'windows-arm64', 'aarch64-pc-windows-msvc'),
                             {'linked': False, 'rust_target': 'aarch64-pc-windows-msvc'})
            path = root / 'crates/avid-media-bridge/build.rs'; path.parent.mkdir(parents=True); path.touch()
            self.assertEqual(release_layout(root, 'windows-arm64', 'aarch64-pc-windows-msvc'),
                             {'linked': True, 'rust_target': 'aarch64-pc-windows-gnullvm'})
            self.assertEqual(release_layout(root, 'linux-arm64', 'aarch64-unknown-linux-gnu')['rust_target'], 'aarch64-unknown-linux-gnu')

    def test_schedule_targets_and_diagnostic_scope_remain_fixed(self):
        workflow = (ROOT / '.github/workflows/weekly-release-smoke.yml').read_text()
        self.assertIn("cron: '17 16 * * 6'", workflow)
        self.assertNotIn('workflow_dispatch:', workflow)
        self.assertNotIn('pull_request:', workflow)
        self.assertNotIn('push:', workflow)
        self.assertNotIn('macos-', workflow)
        self.assertNotIn('upload-artifact', workflow)
        self.assertNotIn('--all-targets', workflow)
        self.assertNotIn('--all-features', workflow)
        self.assertIn("if: github.run_attempt == 1", workflow)
        self.assertIn('--test compatibility --test linked_smoke', workflow)
        self.assertIn('AVID_FFMPEG_PREFIX=', workflow)
        self.assertEqual(set(TARGETS), {'linux-x64', 'linux-arm64', 'windows-x64', 'windows-arm64'})
        for target in ('macos-arm64', 'android-arm64', 'ios-arm64'):
            with self.assertRaises(ValueError): release_layout(ROOT, target, 'unused')


if __name__ == '__main__':
    unittest.main()
