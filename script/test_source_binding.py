"""Publishing must preserve the runtime's exact corresponding-source identity."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import core_release
import core_runtime
import platform_release


class SourceBindingTests(unittest.TestCase):
    def test_platform_preparation_rejects_a_different_source_archive(self):
        spec = json.loads(platform_release.SPEC_PATH.read_text())
        target = 'macos-arm64'
        base = platform_release.artifact_name(spec, target)
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            archive = directory / 'candidate.zip'
            runtime = b'validated runtime'
            with zipfile.ZipFile(archive, 'w') as zipped:
                zipped.writestr(f'packages/{base}.tar.gz', runtime)
                zipped.writestr(f'packages/{base}.tar.gz.sha256',
                    f'{hashlib.sha256(runtime).hexdigest()}  {base}.tar.gz\n')
                zipped.writestr(f'{base}-sources.tar.gz', b'other valid source archive')
            artifact = {'id': 1, 'digest': 'sha256:' + platform_release.digest(archive)}
            files = {'SOURCE.json': json.dumps({'sha256': '0' * 64}).encode()}
            with patch.object(platform_release, 'gh'), \
                    patch.object(platform_release, 'validate_runtime', return_value=files), \
                    patch.object(platform_release, 'validate_sources'):
                with self.assertRaisesRegex(ValueError, 'Source manifest mismatch'):
                    platform_release.prepare_assets(directory, {'head_sha': 'a' * 40, 'id': 1, 'run_attempt': 1},
                        {'id': 3}, artifact, target, spec)
            self.assertFalse((directory / 'release-assets').exists())

    def test_six_target_inventory_rejects_a_different_source_archive(self):
        target = 'macos-arm64'
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            root = directory / target
            root.mkdir()
            archive = root / 'runtime.tar.gz'
            archive.write_bytes(b'validated runtime')
            source = root / 'source.tar.gz'
            source.write_bytes(b'other valid source archive')
            plan = {
                'build_revision': 'a' * 40, 'build_run': 1,
                'repository': 'tlolabs/avid-core', 'build_workflow': 'workflow',
                'build_branch': 'main', 'targets': {target: {
                    'artifact_id': 2, 'artifact_zip_sha256': 'b' * 64,
                    'native_job_id': 3, 'archive': archive.name,
                    'source_archive': source.name, 'sha256': core_runtime.digest(archive),
                    'source_sha256': core_runtime.digest(source),
                    'checksums_sha256': hashlib.sha256(b'checksums').hexdigest(),
                }},
            }
            origin = {'run': {
                'head_sha': plan['build_revision'], 'id': 1, 'event': 'workflow_dispatch',
                'repository': {'full_name': plan['repository']},
                'head_repository': {'full_name': plan['repository']},
                'conclusion': 'success', 'path': 'workflow', 'head_branch': 'main',
            }, 'artifact': {'id': 2, 'digest': 'sha256:' + 'b' * 64},
                'job': {'id': 3, 'conclusion': 'success'}}
            (root / 'origin.json').write_text(json.dumps(origin))
            files = {'SOURCE.json': json.dumps({'sha256': '0' * 64}).encode()}
            build = {key: None for key in ('source_revision', 'recipe', 'version',
                'spec_sha256', 'build_scripts_sha256', 'configure', 'native_host')}
            files.update({'build.json': json.dumps(build).encode(),
                'validation.json': b'{"binary_sha256": {}}',
                'repeat-build.json': b'{}', 'source-provenance.json': b'{}',
                'SHA256SUMS': b'checksums'})
            with patch.object(core_release, 'validate_runtime', return_value=files), \
                    patch.object(core_release, 'validate_sources'):
                with self.assertRaisesRegex(ValueError, 'Source manifest mismatch'):
                    core_release.inventory(plan, directory)


if __name__ == '__main__':
    unittest.main()
