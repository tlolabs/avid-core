"""Security and idempotency checks for the independent platform publisher."""
import copy
import hashlib
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

    def test_existing_untagged_draft_resumes_by_release_id(self):
        tag = 'platform-ffmpeg-9.0.1-r7-linux-arm64-run123'
        draft = {
            'id': 42, 'tag_name': tag, 'name': 'title', 'body': 'notes',
            'target_commitish': 'main', 'prerelease': False, 'draft': True,
            'assets': [],
        }
        with patch.object(publisher, 'find_release', return_value=draft), \
                patch.object(publisher, 'release_api') as api:
            self.assertEqual(
                publisher.create_or_get_release(tag, 'title', 'notes', self.sha),
                draft,
            )
            api.assert_not_called()

    def test_empty_draft_from_previous_attempt_is_repaired_without_duplication(self):
        tag, title, notes = publisher.release_identity(
            self.run, 'linux-arm64', self.spec, {'spec_sha256': 'c' * 64},
        )
        draft = {
            'id': 42, 'tag_name': tag, 'name': title,
            'body': notes.replace(publisher.PUBLICATION_NOTE, ''),
            'target_commitish': self.sha, 'prerelease': False,
            'draft': True, 'assets': [],
        }
        repaired = {**draft, 'body': notes, 'target_commitish': 'main'}
        with patch.object(publisher, 'find_release', return_value=draft), \
                patch.object(publisher, 'release_api', return_value=repaired) as api:
            self.assertEqual(
                publisher.create_or_get_release(tag, title, notes, self.sha),
                repaired,
            )
            self.assertEqual(api.call_args.args[:2],
                             ('PATCH', 'repos/tlolabs/avid-core/releases/42'))

    def test_asset_digest_must_match_exact_bytes(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / 'runtime.tar.gz'
            path.write_bytes(b'validated')
            asset = {
                'name': path.name,
                'state': 'uploaded',
                'size': path.stat().st_size,
                'digest': 'sha256:' + publisher.digest(path),
            }
            publisher.verify_asset(asset, path)
            with self.assertRaises(ValueError):
                publisher.verify_asset({**asset, 'digest': 'sha256:' + '0' * 64}, path)

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
            with patch.object(publisher, 'get_release',
                              return_value={'id': 1, 'draft': False, 'assets': []}), \
                    patch.object(publisher, 'release_api') as api:
                with self.assertRaises(ValueError):
                    publisher.publish_assets({'id': 1, 'draft': False}, 'tag', output)
                api.assert_not_called()

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
            release = {'id': 1, 'draft': False, 'assets': [{'name': name} for name in names]}
            with patch.object(publisher, 'get_release', return_value=release), \
                    patch.object(publisher, 'verify_asset') as verify, \
                    patch.object(publisher, 'release_api') as api:
                self.assertEqual(
                    publisher.publish_assets(release, 'tag', output),
                    release,
                )
                self.assertEqual(verify.call_count, 2 * len(names))
                api.assert_not_called()

    def test_release_tag_is_bound_to_attested_publisher_commit(self):
        tag = 'platform-ffmpeg-9.0.1-r7-linux-arm64-run123'
        revision = 'c' * 40
        ref = {'object': {'type': 'commit', 'sha': revision}}
        with patch.object(publisher, 'release_api', return_value=ref) as api:
            publisher.ensure_tag(tag, revision)
            self.assertEqual(api.call_count, 1)
            with self.assertRaisesRegex(ValueError, 'attested publisher revision'):
                publisher.ensure_tag(tag, 'd' * 40)

    def test_prepared_provenance_binds_target_build_and_runtime_bytes(self):
        run = {**self.run, 'run_attempt': 1}
        target = 'linux-arm64'
        base = publisher.artifact_name(self.spec, target)
        publisher_revision = 'c' * 40
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            output = directory / 'release-assets'
            output.mkdir()
            runtime = output / (base + '.tar.gz')
            sources = output / (base + '-sources.tar.gz')
            runtime.write_bytes(b'validated runtime')
            sources.write_bytes(b'corresponding source')
            (output / (runtime.name + '.sha256')).write_text(
                f'{publisher.digest(runtime)}  {runtime.name}\n')
            provenance = {
                'schema': 1, 'kind': 'direct-download-platform',
                'repository': publisher.REPOSITORY, 'target': target,
                'build_run': run['id'], 'build_attempt': run['run_attempt'],
                'build_revision': run['head_sha'], 'native_job_id': self.job['id'],
                'artifact_id': self.artifact['id'],
                'artifact_zip_sha256': self.artifact['digest'].removeprefix('sha256:'),
                'spec_sha256': hashlib.sha256(publisher.SPEC_PATH.read_bytes()).hexdigest(),
                'runtime_archive': runtime.name, 'runtime_sha256': publisher.digest(runtime),
                'source_archive': sources.name, 'source_sha256': publisher.digest(sources),
            }
            manifest = output / 'platform-release.json'
            manifest.write_text(json.dumps(provenance))
            names = {runtime.name, sources.name, runtime.name + '.sha256', 'platform-release.json'}
            checksums = output / 'SHA256SUMS'
            checksums.write_text(''.join(
                f'{publisher.digest(output / name)}  {name}\n' for name in sorted(names)))
            tag, title, notes = publisher.release_identity(run, target, self.spec, provenance)
            identity = {
                'schema': 1, 'source_run_id': run['id'], 'target': target,
                'build_revision': run['head_sha'], 'publisher_revision': publisher_revision,
                'tag': tag, 'title': title, 'notes': notes,
                'assets_sha256': {p.name: publisher.digest(p) for p in output.iterdir()},
            }
            identity_path = directory / 'identity.json'
            identity_path.write_text(json.dumps(identity))
            with patch.dict('os.environ', {'GITHUB_SHA': publisher_revision}), \
                    patch.object(publisher, 'get_origin', return_value=(run, self.job, self.artifact)), \
                    patch.object(publisher, 'validate_runtime'), \
                    patch.object(publisher, 'validate_sources'):
                self.assertEqual(publisher.verify_prepared(directory)[0], identity)
                for field, wrong in [('target', 'windows-arm64'),
                                     ('build_revision', 'd' * 40),
                                     ('spec_sha256', '0' * 64),
                                     ('runtime_archive', 'unexpected.tar.gz')]:
                    changed = {**provenance, field: wrong}
                    manifest.write_text(json.dumps(changed))
                    identity['assets_sha256']['platform-release.json'] = publisher.digest(manifest)
                    identity_path.write_text(json.dumps(identity))
                    with self.subTest(field=field), self.assertRaises(ValueError):
                        publisher.verify_prepared(directory)
                manifest.write_text(json.dumps(provenance))
                identity['assets_sha256']['platform-release.json'] = publisher.digest(manifest)
                identity_path.write_text(json.dumps(identity))
                runtime.write_bytes(b'altered runtime')
                with self.assertRaises(ValueError):
                    publisher.verify_prepared(directory)


if __name__ == '__main__':
    unittest.main()
