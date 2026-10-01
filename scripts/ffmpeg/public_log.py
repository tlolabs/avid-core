"""Redact machine-specific path prefixes before build evidence leaves the runner.

This is path minimization, not a filter for arbitrary secret-bearing data. Callers
must never send environment dumps, raw config.log, command-line/process inventories
or unscoped handle data through it. Lifecycle diagnostics use a separate allowlist.
"""
import os
from pathlib import Path
import re
import sys
import tempfile


def sanitize(text):
    roots = [(str(Path(__file__).resolve().parents[2]), '<WORKSPACE>'),
             (os.environ.get('GITHUB_WORKSPACE'), '<WORKSPACE>'),
             (os.environ.get('RUNNER_TEMP'), '<TEMP>'),
             (tempfile.gettempdir(), '<TEMP>'),
             (os.environ.get('USERPROFILE'), '<USERPROFILE>'),
             (os.environ.get('HOME'), '<USERPROFILE>')]
    replacements = []
    for root, symbol in roots:
        if not root or root in {'/', '\\'}:
            continue
        variants = {root, root.replace('\\', '/'), root.replace('/', '\\')}
        for value in list(variants):
            if re.match(r'^[A-Za-z]:/', value):
                variants.add('/'+value[0].lower()+value[2:])
        for value in variants:
            replacements.extend([(value, symbol), (value.replace('\\', '\\\\'), symbol)])
    for root, symbol in sorted(replacements, key=lambda item: len(item[0]), reverse=True):
        text = re.sub(re.escape(root), lambda _: symbol, text, flags=re.IGNORECASE)
    # Cover native paths printed by programs launched from MSYS, whose profile and
    # temporary-directory conventions can differ from the Python interpreter's.
    text = re.sub(r'(?i)(?:[a-z]:|/[a-z])[/\\]+Users[/\\]+[^/\\\s"\']+', '<USERPROFILE>', text)
    text = re.sub(r'(?i)/(?:Users|home)/[^/\s"\']+', '<USERPROFILE>', text)
    text = re.sub(r'(?i)(?:[a-z]:|/[a-z])[/\\]+a[/\\]+_temp', '<TEMP>', text)
    return text


if __name__ == '__main__':
    for line in sys.stdin:
        print(sanitize(line), end='', flush=True)
