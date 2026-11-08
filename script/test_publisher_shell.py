"""Ensure reusable-workflow inputs stay literal shell arguments."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parent.parent


class PublisherShellTests(unittest.TestCase):
    def test_inputs_are_passed_through_quoted_environment_expansions(self):
        workflow = (ROOT / '.github/workflows/platform-publisher.yml').read_text()
        self.assertIn('SOURCE_RUN_ID: ${{ inputs.source_run_id }}', workflow)
        self.assertIn('PLATFORM_TARGET: ${{ inputs.target }}', workflow)
        self.assertIn('--run-id "$SOURCE_RUN_ID" --target "$PLATFORM_TARGET"', workflow)
        commands = self.prepare_commands(workflow)
        self.assertEqual(len(commands), 1)
        self.assertNotIn('${{', commands[0])

    @staticmethod
    def prepare_commands(workflow):
        prefix = 'run: python3 script/platform_release.py --run-id '
        return [line.strip()[len('run: '):] for line in workflow.splitlines()
                if line.strip().startswith(prefix)]

    def test_normal_and_hostile_values_remain_literal_arguments(self):
        workflow = (ROOT / '.github/workflows/platform-publisher.yml').read_text()
        commands = self.prepare_commands(workflow)
        self.assertEqual(len(commands), 1)
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / 'injected'
            # Capture the actual workflow command's argv; never run the publisher.
            capture = Path(directory) / 'python3'
            capture.write_text(
                f'#!{sys.executable}\nimport json,sys\n'
                'print(json.dumps(sys.argv[1:]))\n'
            )
            capture.chmod(0o755)
            cases = [
                ('123456', 'x86_64-apple-darwin'),
                (f"123'; touch '{marker}'; #", f"$(touch '{marker}')\n--unexpected"),
                (f'123"; touch "{marker}"; #', f"`touch '{marker}'` * ? [abc]"),
            ]
            for run_id, target in cases:
                with self.subTest(run_id=run_id, target=target):
                    environment = os.environ.copy()
                    environment.update(
                        SOURCE_RUN_ID=run_id,
                        PLATFORM_TARGET=target,
                        PATH=directory + os.pathsep + environment.get('PATH', ''),
                    )
                    result = subprocess.run(
                        ['/bin/sh', '-c', commands[0]],
                        check=True,
                        capture_output=True,
                        text=True,
                        env=environment,
                        cwd=directory,
                    )
                    self.assertEqual(
                        json.loads(result.stdout),
                        ['script/platform_release.py', '--run-id', run_id,
                         '--target', target, '--prepare-dir', 'build/platform-publication'],
                    )
                    self.assertFalse(marker.exists())


if __name__ == '__main__':
    unittest.main()
