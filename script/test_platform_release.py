"""Security and idempotency checks for the independent platform publisher."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import platform_release as publisher


class PlatformReleaseTests(unittest.TestCase):
    def setUp(self):
        self.spec = json.loads(publisher.SPEC_PATH.read_text())
        self.sha = 'a' * 40
        self.run = {
            'id': 123,
            'head_sha': self.sha,
            'head_branch': 'main',
            'event': 'push',
            'path': '.github/workflows/ffmpeg-independent.yml',
            'repository': {'full_name': publisher.REPOSITORY},
            'head_repository': {'full_name': publisher.REPOSITORY},
        }
        self.job = {
            'id': 456,
            'name': 'build (linux-arm64) / build (linux-arm64)',
            'conclusion': 'success',
            'steps': [
                {'name': 'Build and validate native Unix runtime', 'conclusion': 'success'},
                {'name': publisher.UPLOAD_STEP, 'conclusion': 'success'},
            ],
        }
        self.artifact = {
            'id': 789,
            'name': 'core-runtime-candidate-linux-arm64',
            'expired': False,
            'digest': 'sha256:' + 'b' * 64,
            'workflow_run': {'id': 123, 'head_sha': self.sha},
        }

    def test_completed_target_is_eligible_while_other_targets_are_queued(self):
        self.assertEqual(
            publisher.validated_origin(self.run, [self.job], [self.artifact],
                                       'linux-arm64', self.spec),
            (self.job, self.artifact),
        )

    def test_failed_or_untrusted_candidate_cannot_publish(self):
        for changed, value in [
            ('run', {'head_branch': 'feature'}),
            ('job', {'conclusion': 'failure'}),
            ('artifact', {'digest': 'sha256:bad'}),
        ]:
            run, job, artifact = copy.deepcopy((self.run, self.job, self.artifact))
            {'run': run, 'job': job, 'artifact': artifact}[changed].update(value)
            with self.subTest(changed=changed), self.assertRaises(ValueError):
                publisher.validated_origin(run, [job], [artifact], 'linux-arm64', self.spec)

    def test_direct_download_tag_cannot_match_authenticated_runtime_tag(self):
        provenance = {'spec_sha256': 'c' * 64}
        tag, title, notes = publisher.release_identity(self.run, 'linux-arm64',
                                                        self.spec, provenance)
        self.assertEqual(tag, 'platform-ffmpeg-9.0.1-r7-linux-arm64-run123')
        self.assertIn('linux-arm64', title)
        self.assertIn(self.sha, notes)
        self.assertNotEqual(tag, 'ffmpeg-9.0.1-r7.1')

    def test_published_release_with_missing_asset_fails_closed(self):
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp) / 'assets'
            output.mkdir()
            for name in ('avid-ffmpeg-runtime.tar.gz',
                         'avid-ffmpeg-runtime-sources.tar.gz',
                         'avid-ffmpeg-runtime.tar.gz.sha256',
                         'platform-release.json', 'SHA256SUMS'):
                (output / name).write_text(name)
            (output / 'platform-release.json').write_text(json.dumps({
                'runtime_archive': 'avid-ffmpeg-runtime.tar.gz',
                'source_archive': 'avid-ffmpeg-runtime-sources.tar.gz',
            }))
            with patch.object(publisher, 'gh_json', return_value={'draft': False, 'assets': []}), \
                    patch.object(publisher, 'gh') as gh:
                with self.assertRaises(ValueError):
                    publisher.publish_assets({'draft': False}, 'tag', output, Path(temp))
                gh.assert_not_called()

    def test_existing_published_assets_are_verified_without_upload(self):
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp) / 'assets'
            output.mkdir()
            names = ('avid-ffmpeg-runtime.tar.gz',
                     'avid-ffmpeg-runtime-sources.tar.gz',
                     'avid-ffmpeg-runtime.tar.gz.sha256',
                     'platform-release.json', 'SHA256SUMS')
            for name in names:
                (output / name).write_text(name)
            (output / 'platform-release.json').write_text(json.dumps({
                'runtime_archive': 'avid-ffmpeg-runtime.tar.gz',
                'source_archive': 'avid-ffmpeg-runtime-sources.tar.gz',
            }))
            release = {'draft': False, 'assets': [{'name': name} for name in names]}
            with patch.object(publisher, 'gh_json', return_value=release), \
                    patch.object(publisher, 'verify_asset') as verify, \
                    patch.object(publisher, 'gh') as gh:
                self.assertEqual(
                    publisher.publish_assets(release, 'tag', output, Path(temp)),
                    release,
                )
                self.assertEqual(verify.call_count, len(names))
                gh.assert_not_called()


if __name__ == '__main__':
    unittest.main()
