"""Check diagnostic spawn/wait/pipe-close/drop evidence before runtime rename."""
import json
import re
import sys
from pathlib import Path


def verify(text):
    active = {}
    completed = 0
    boundaries = 0
    for line in text.splitlines():
        match = re.search(r"(core_child_\w+|fixture_child_\w+) pid=(\d+)(.*)", line)
        if match:
            event, pid, detail = match.groups()
            if event.endswith("_spawn"):
                assert pid not in active, f"Reused live child PID {pid}"
                active[pid] = set()
            else:
                assert pid in active, f"Unmatched child event {event} {pid}"
                if event == "core_child_readers_joined":
                    assert "status=Ok(" in detail, f"Child wait failed: {line}"
                    active[pid].add("readers")
                elif event == "core_child_final_wait":
                    assert "result=Ok(" in detail, f"Final child wait failed: {line}"
                    active[pid].add("wait")
                elif event == "core_child_resources_dropped":
                    assert active[pid] == {"wait", "readers"}, f"Incomplete child cleanup: {pid}: {active[pid]}"
                    del active[pid]
                    completed += 1
                elif event == "fixture_child_wait_and_drop":
                    assert "status=ExitStatus(" in detail, f"Missing fixture child exit: {line}"
                    del active[pid]
                    completed += 1
                else:
                    raise AssertionError(f"Unknown child event: {event}")
        if "runtime_rename_begin" in line:
            assert not active, f"Children/resources remain at rename: {active}"
            assert completed > 3, "Core child tracing was not enabled"
            boundaries += 1
    assert boundaries == 1, f"Expected one rename boundary, got {boundaries}"
    assert not active, f"Children/resources remain at test exit: {active}"
    return {"completed_children": completed, "rename_boundaries": boundaries, "unreleased_children": 0}


if __name__ == "__main__":
    for path in sys.argv[1:]:
        print(json.dumps({"log": Path(path).name, **verify(Path(path).read_text(encoding="utf-8-sig"))}))
