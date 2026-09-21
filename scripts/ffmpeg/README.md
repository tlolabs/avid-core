# Diagnostic tools only

These pinned FFmpeg build and validation tools are retained research, not the
AVID Core dependency or release mechanism. They impose no host packaging,
signing, notarization, updater or UI gates. There is no publish command.

Distribution-only helpers were removed from this directory and preserved as
non-executable text under `docs/ffmpeg/historical/`. Hosts must replace imports
or calls to acquire.py, host.py and package.py with host-owned packaging.
See `docs/integration.md` and the two host handoffs.

Run retained research unit tests with:

    python3 -m unittest discover -s scripts/ffmpeg -p 'test_*.py'

Build experiments remain explicit/manual: `bash scripts/ffmpeg/ci.sh <target>`.
