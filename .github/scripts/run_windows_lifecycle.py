"""Run diagnostics with an allowlist before anything reaches logs or disk.
Raw subprocess output remains in memory; no diagnostic artifacts are uploaded.
"""
import collections
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from verify_child_trace import verify

PREFIXES = ('core_child_', 'fixture_child_', 'runtime_rename_begin',
            'runtime_replacement_failed', 'scoped_handle', 'resource_owner_',
            'native_owner ', 'test_resource=', 'RmStartSession=', 'RmRegisterResources=', 'RmGetList')
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
               AVID_LOCK_DIAGNOSTIC=str(root/'owners.exe'),
               AVID_HANDLE_DIAGNOSTIC=str(root/'handle/handle64.exe'), RUST_BACKTRACE='0')
    command = ['cargo', 'test', '--locked', '--features', 'lifecycle-diagnostics', '--test', 'runtime_contract']
    code, output = captured(command+['--no-run'], env=env)
    print(json.dumps({'compile_exit': code}), flush=True)
    if code:
        # Rust error codes only; never dump a compiler diagnostic path or source line.
        print(json.dumps({'compiler_error_codes': re.findall(r'error\[(E\d+)\]', output)}))
        return code
    result = 0
    for iteration in range(1, 501):
        code, output = captured(command+['installed_runtime_render_replacement_rollback_and_cleanup',
                                '--', '--ignored', '--exact', '--nocapture', '--test-threads=1'], env=env)
        try:
            evidence = verify(output)
            trace_ok = True
        except AssertionError:
            evidence = {'trace_verified': False}
            trace_ok = False
        if iteration % 25 == 0 or code or not trace_ok:
            print(json.dumps({'iteration': iteration, 'exit': code, **evidence}), flush=True)
        if code or not trace_ok:
            safe = public_lines(output)
            # Completed child evidence is summarized above; print last child events plus lock evidence.
            children = [s for s in safe if s.startswith(('core_child_', 'fixture_child_'))]
            for line in children[-8:] + [s for s in safe if not s.startswith(('core_child_', 'fixture_child_'))]:
                print(line, flush=True)
            print(json.dumps({'failed_test': 'installed_runtime_render_replacement_rollback_and_cleanup',
                              'raw_output_withheld': True}), flush=True)
            result = code or 1
            break
    # Independent control, not a retry of the failed qualification operation.
    code, output = captured([str(root/'exit-control.exe'), str(root/'runtime/ffmpeg.exe'),
                             str(Path(os.environ['RUNNER_TEMP'])/'avid-exit-control')])
    counts = collections.Counter()
    for line in output.splitlines():
        m = re.fullmatch(r'mode=(\d+) iteration=(\d+) exit=(\d+) waited=1 open_handles_rename=(\d+) closed_handles_rename=(\d+)', line)
        if m:
            mode, iteration, status, opened, closed = map(int, m.groups())
            counts[(mode, opened, closed)] += 1
        elif re.fullmatch(r'(?:setup|spawn|terminate|wait|cleanup) error=\d+(?: result=\d+)?', line):
            print('control_'+line, flush=True)
    print(json.dumps({'control_exit': code, 'control_counts': [dict(mode=k[0], open_error=k[1], closed_error=k[2], count=v) for k,v in counts.items()]}), flush=True)
    return result or code


if __name__ == '__main__':
    try:
        sys.exit(run())
    except Exception as error:
        # Exceptions may carry absolute paths or raw commands; publish only the type.
        print(json.dumps({'diagnostic_error_type': type(error).__name__}), flush=True)
        sys.exit(2)
