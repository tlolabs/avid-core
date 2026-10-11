import unittest

from update_ci_failure_report import issue_state, merge_alerts, merge_entries


class FailureReportStateTests(unittest.TestCase):
    def test_new_failure_is_marked_new(self):
        run = {
            "workflow_id": 1,
            "workflow": "shared-core",
            "branch": "main",
            "event": "push",
            "conclusion": "failure",
            "url": "https://github.com/tlolabs/avid-core/actions/runs/1",
            "failures": [],
        }
        result = merge_entries({}, {"1:push:main": run}, "2026-10-11T00:00:00+00:00")
        self.assertEqual(result["1:push:main"]["status"], "NEW")

    def test_repeat_failure_stays_persistent(self):
        previous = {
            "1:push:main": {
                "workflow_id": 1,
                "workflow": "shared-core",
                "branch": "main",
                "event": "push",
                "conclusion": "failure",
                "url": "old",
                "status": "NEW",
                "first_seen": "yesterday",
            }
        }
        run = {
            "workflow_id": 1,
            "workflow": "shared-core",
            "branch": "main",
            "event": "push",
            "conclusion": "failure",
            "url": "today",
            "failures": [],
        }
        result = merge_entries(previous, {"1:push:main": run}, "today")
        self.assertEqual(result["1:push:main"]["status"], "PERSISTENT")
        self.assertEqual(result["1:push:main"]["first_seen"], "yesterday")

    def test_success_resolves_previously_open_failure(self):
        previous = {
            "1:push:main": {
                "workflow_id": 1,
                "workflow": "shared-core",
                "branch": "main",
                "event": "push",
                "conclusion": "failure",
                "url": "failed",
                "status": "PERSISTENT",
                "first_seen": "yesterday",
            }
        }
        run = {
            "workflow_id": 1,
            "workflow": "shared-core",
            "branch": "main",
            "event": "push",
            "conclusion": "success",
            "url": "passed",
        }
        result = merge_entries(previous, {"1:push:main": run}, "today")
        self.assertEqual(result["1:push:main"]["status"], "RESOLVED")
        self.assertEqual(result["1:push:main"]["resolved_url"], "passed")

    def test_nonblocking_failed_step_remains_reported(self):
        previous = {}
        run = {
            "workflow_id": 1,
            "workflow": "source-release",
            "branch": "main",
            "event": "push",
            "conclusion": "success",
            "url": "today",
            "failures": [{"job": "stable-source", "steps": ["Create optional provenance"], "url": "today"}],
        }
        result = merge_entries(previous, {"1:push:main": run}, "today")
        self.assertEqual(result["1:push:main"]["status"], "NONBLOCKING NEW")

    def test_cancelled_run_does_not_create_a_failure(self):
        run = {
            "workflow_id": 1,
            "workflow": "shared-core",
            "branch": "main",
            "event": "push",
            "conclusion": "cancelled",
            "url": "today",
        }
        self.assertEqual(merge_entries({}, {"1:push:main": run}, "today"), {})

    def test_issue_state_without_marker_starts_empty(self):
        self.assertEqual(issue_state("plain issue body"), {"entries": {}})

    def test_closed_security_alert_is_marked_resolved(self):
        previous = {
            "Dependabot:7": {
                "key": "Dependabot:7",
                "kind": "Dependabot",
                "label": "example: vulnerability",
                "url": "alert",
                "status": "PERSISTENT",
                "first_seen": "yesterday",
            }
        }
        current, unavailable = merge_alerts(
            previous,
            [{"available": True, "kind": "Dependabot", "items": []}],
            "today",
        )
        self.assertFalse(unavailable)
        self.assertEqual(current["Dependabot:7"]["status"], "RESOLVED")


if __name__ == "__main__":
    unittest.main()
