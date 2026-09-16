"""Run diagnostics with an allowlist before anything reaches logs or disk.
Raw subprocess output remains in memory; no diagnostic artifacts are uploaded.
"""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from verify_child_trace import verify

PREFIXES = ('core_child_', 'fixture_child_', 'runtime_rename_begin',
            'runtime_replacement_failed', 'runtime_directory_', 'scoped_handle', 'resource_owner_',
            'native_owner ', 'native_detail ', 'test_resource=', 'RmStartSession=', 'RmRegisterResources=', 'RmGetList')
SAFE = re.compile(r'[A-Za-z0-9_ .,:=<>?()/\-]+\Z')


def public_lines(raw):
    lines = []
    for line in raw.splitlines():
        line = line.strip()
        if line.startswith(PREFIXES) and SAFE.fullmatch(line):
            # The only allowed path is a symbolic runtime resource. Reject any other slash.
            check = re.sub(r'<RUNTIME>/(?:ffmpeg\.exe|ffprobe\.exe|spec\.json|build\.json|<file>)', '<RUNTIME>', line)
            if '/' not in check and '\\' not in check and not re.search(r'[A-Za-z]:', check):
                lines.append(line)
    return lines


def captured(args, **kwargs):
    result = subprocess.run(args, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            encoding='utf-8', errors='replace', **kwargs)
    return result.returncode, result.stdout


def run():
    root = Path('.ffmpeg-work/locks').resolve()
    env = dict(os.environ, AVID_RUNTIME_DIRECTORY=str(root/'runtime'),
               RUST_BACKTRACE='0')
    command = ['cargo', 'test', '--locked', '--features', 'lifecycle-diagnostics', '--test', 'runtime_contract']
    code, output = captured(command+['--no-run'], env=env)
    print(json.dumps({'compile_exit': code}), flush=True)
    if code:
        # Rust error codes only; never dump a compiler diagnostic path or source line.
        print(json.dumps({'compiler_error_codes': re.findall(r'error\[(E\d+)\]', output)}))
        return code
    code, output = captured(['cargo', 'test', '--locked', '--features', 'lifecycle-diagnostics',
                             '--test', 'runtime_directory', '--', '--test-threads=1'], env=env)
    print(json.dumps({'controlled_lock_tests_exit': code}), flush=True)
    if code: return code
    recovered = 0
    total = 0
    for test in ['legacy_discovery_then_raw_cleanup']:
        for iteration in range(1, 1001):
            code, output = captured(command+[test, '--', '--ignored', '--exact', '--nocapture', '--test-threads=1'], env=env)
            recovered += output.count('runtime_directory_recovered')
            try:
                # Reuse the strict child-resource verifier at the cleanup boundary.
                evidence = verify(output.replace('runtime_directory_cleanup_begin', 'runtime_rename_begin'))
                evidence['cleanup_boundaries'] = evidence.pop('rename_boundaries')
                trace_ok = True
            except AssertionError:
                evidence = {'trace_verified': False}
                trace_ok = False
            total += 1
            if iteration % 25 == 0 or code or not trace_ok:
                print(json.dumps({'test': test, 'iteration': iteration, 'total_cycles': total, 'exit': code,
                                  'recovered_directory_operations': recovered, **evidence}), flush=True)
            if code or not trace_ok:
                safe = public_lines(output)
                children = [s for s in safe if s.startswith(('core_child_', 'fixture_child_'))]
                for line in children[-8:] + [s for s in safe if not s.startswith(('core_child_', 'fixture_child_'))]:
                    print(line, flush=True)
                print(json.dumps({'failed_test': test, 'raw_output_withheld': True,
                                  'test_line': re.findall(r'runtime_contract\.rs:(\d+):\d+', output),
                                  'os_error_codes': re.findall(r'(?:os error |code: )(\d+)', output),
                                  'io_error_kinds': re.findall(r'kind: (\w+)', output)}), flush=True)
                return code or 1
    print(json.dumps({'historical_reproducer_completed': True, 'consecutive_cycles': total,
                      'recovered_directory_operations': recovered}), flush=True)
    return 0


if __name__ == '__main__':
    try:
        sys.exit(run())
    except Exception as error:
        # Exceptions may carry absolute paths or raw commands; publish only the type.
        print(json.dumps({'diagnostic_error_type': type(error).__name__}), flush=True)
        sys.exit(2)
