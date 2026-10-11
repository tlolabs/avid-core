#!/usr/bin/env python3
"""Refresh the consolidated CI/CD issue from workflow runs and security alerts."""

import json
import re
import subprocess
from datetime import datetime, timezone

REPOSITORY = "tlolabs/avid-core"
ISSUE_NUMBER = "9"
STATE_PATTERN = re.compile(r"<!-- ci-failure-state: (.*?) -->", re.DOTALL)
FAILURE_CONCLUSIONS = {"action_required", "cancelled", "failure", "startup_failure", "timed_out"}
SUCCESS_CONCLUSIONS = {"success", "neutral", "skipped"}
ACTIVE_STATUSES = {"NEW", "PERSISTENT", "NONBLOCKING NEW", "NONBLOCKING PERSISTENT"}
INCONCLUSIVE_CONCLUSIONS = {"cancelled"}
TRACKED_WORKFLOWS = {
    "shared-core",
    "Windows lrintf compiler regression",
    "FFmpeg native candidate verification",
    "Promote verified Core runtime",
    "source-release",
    "Linux lifecycle race regression",
    "Windows runtime lifecycle stress",
    "Recipe 6 failure investigation",
    "Windows compiler isolation",
    "Dependabot Updates",
}


def gh_json(arguments, unavailable_statuses=()):
    result = subprocess.run(
        ["gh", "api", "--method", "GET", *arguments],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode:
        error = result.stderr.strip()
        if any(f"HTTP {status}" in error for status in unavailable_statuses):
            return None, error
        raise RuntimeError(error or f"gh api failed: {' '.join(arguments)}")
    return json.loads(result.stdout), None


def issue_state(body):
    match = STATE_PATTERN.search(body or "")
    return json.loads(match.group(1)) if match else {"entries": {}}


def merge_entries(previous, latest, now):
    entries = {
        key: dict(value)
        for key, value in previous.items()
        if value.get("workflow") in TRACKED_WORKFLOWS
        and value.get("conclusion") not in INCONCLUSIVE_CONCLUSIONS
    }
    for key, run in latest.items():
        if run["conclusion"] in INCONCLUSIVE_CONCLUSIONS:
            continue
        old = entries.get(key)
        reportable = run["conclusion"] not in SUCCESS_CONCLUSIONS or bool(run.get("failures"))
        if not reportable:
            if old and old["status"] in ACTIVE_STATUSES:
                entries[key] = {
                    **old,
                    "status": "RESOLVED",
                    "resolved_at": now,
                    "resolved_url": run["url"],
                }
            continue

        still_open = old and old["status"] in ACTIVE_STATUSES
        nonblocking = run["conclusion"] in SUCCESS_CONCLUSIONS
        entries[key] = {
            **run,
            "status": (
                "NONBLOCKING PERSISTENT" if still_open else "NONBLOCKING NEW"
            ) if nonblocking else ("PERSISTENT" if still_open else "NEW"),
            "first_seen": old["first_seen"] if still_open else now,
            "last_seen": run.get("created_at", now),
        }

    resolved = sorted(
        (entry for entry in entries.values() if entry["status"] == "RESOLVED"),
        key=lambda entry: entry.get("resolved_at", ""),
        reverse=True,
    )[:100]
    active = [entry for entry in entries.values() if entry["status"] != "RESOLVED"]
    return {
        f"{entry['workflow_id']}:{entry['event']}:{entry['branch']}": entry
        for entry in [*active, *resolved]
    }


def markdown_cell(value):
    return str(value).replace("|", "\\|").replace("\r", " ").replace("\n", " ")


def latest_completed_runs():
    latest = {}
    page = 1
    while True:
        result, _ = gh_json(
            [
                f"repos/{REPOSITORY}/actions/runs",
                "-f",
                "per_page=100",
                "-f",
                f"page={page}",
            ]
        )
        runs = result["workflow_runs"]
        for run in runs:
            if run["status"] != "completed":
                continue
            workflow = "Dependabot Updates" if run["event"] == "dynamic" else run["name"]
            if workflow not in TRACKED_WORKFLOWS:
                continue
            key = f"{run['workflow_id']}:{run['event']}:{run.get('head_branch') or ''}"
            if key not in latest:
                latest[key] = {
                    "workflow_id": run["workflow_id"],
                    "workflow": workflow,
                    "branch": run.get("head_branch") or "(unknown)",
                    "event": run["event"],
                    "conclusion": run.get("conclusion") or "unknown",
                    "url": run["html_url"],
                    "run_id": run["id"],
                    "created_at": run["created_at"],
                }
        if len(runs) < 100:
            break
        page += 1
    return latest


def failure_details(run):
    result, _ = gh_json(
        [
            f"repos/{REPOSITORY}/actions/runs/{run['run_id']}/jobs",
            "-f",
            "per_page=100",
        ]
    )
    details = []
    for job in result["jobs"]:
        failed_steps = [
            step["name"]
            for step in job.get("steps", [])
            if step.get("outcome") in FAILURE_CONCLUSIONS
            or step.get("conclusion") in FAILURE_CONCLUSIONS
        ]
        if job.get("conclusion") not in FAILURE_CONCLUSIONS and not failed_steps:
            continue
        details.append(
            {
                "job": job["name"],
                "steps": failed_steps,
                "url": job["html_url"],
            }
        )
    if details or run["conclusion"] in SUCCESS_CONCLUSIONS:
        return details
    return [{"job": "Workflow failed without a failed job detail", "steps": [], "url": run["url"]}]


def security_alerts(endpoint, kind):
    result, error = gh_json(
        [f"repos/{REPOSITORY}/{endpoint}", "-f", "state=open", "-f", "per_page=100"],
        unavailable_statuses=(403, 404),
    )
    if error:
        return {"available": False, "detail": error, "kind": kind, "items": []}

    items = []
    for alert in result:
        if kind == "Dependabot":
            package = alert.get("dependency", {}).get("package", {}).get("name", "unknown package")
            advisory = alert.get("security_advisory", {})
            summary = advisory.get("summary", "Unspecified dependency vulnerability")
            severity = advisory.get("severity", "unknown")
            url = alert.get("html_url", "")
            label = f"{package}: {summary} ({severity})"
        elif kind == "Code scanning":
            rule = alert.get("rule", {})
            label = f"{rule.get('security_severity_level', 'unknown')}: {rule.get('description', 'Code scanning finding')}"
            url = alert.get("html_url", "")
        else:
            label = alert.get("secret_type_display_name", "Secret scanning finding")
            url = alert.get("html_url", "")
        identity = alert.get("number") or url or label
        items.append({"key": f"{kind}:{identity}", "label": label, "url": url})
    return {"available": True, "detail": "", "kind": kind, "items": items}


def merge_alerts(previous, responses, now):
    alerts = {key: dict(value) for key, value in previous.items()}
    unavailable = []
    for response in responses:
        if not response["available"]:
            unavailable.append(response)
            continue

        current = {item["key"]: item for item in response["items"]}
        for key, old in list(alerts.items()):
            if old["kind"] == response["kind"] and old["status"] in {"NEW", "PERSISTENT"} and key not in current:
                alerts[key] = {**old, "status": "RESOLVED", "resolved_at": now}

        for key, item in current.items():
            old = alerts.get(key)
            still_open = old and old["status"] in {"NEW", "PERSISTENT"}
            alerts[key] = {
                **item,
                "kind": response["kind"],
                "status": "PERSISTENT" if still_open else "NEW",
                "first_seen": old["first_seen"] if still_open else now,
                "last_seen": old["last_seen"] if still_open else now,
            }

    resolved = sorted(
        (item for item in alerts.values() if item["status"] == "RESOLVED"),
        key=lambda item: item.get("resolved_at", ""),
        reverse=True,
    )[:100]
    active = [item for item in alerts.values() if item["status"] != "RESOLVED"]
    return {
        item["key"]: item for item in [*active, *resolved]
    }, unavailable


def render_body(state, alerts, unavailable_alerts, refreshed_at):
    entries = list(state.values())
    unresolved = sorted(
        (entry for entry in entries if entry["status"] in ACTIVE_STATUSES),
        key=lambda entry: (entry["status"], entry["workflow"], entry["branch"]),
    )
    resolved = sorted(
        (entry for entry in entries if entry["status"] == "RESOLVED"),
        key=lambda entry: entry.get("resolved_at", ""),
        reverse=True,
    )[:20]

    lines = [
        "# CI/CD failure report",
        "",
        "This issue is refreshed automatically after CI/CD workflow runs. Diagnostic failures are tracked here and do not block unrelated development.",
        "",
        f"Last refreshed: {refreshed_at}",
        "",
        "## Unresolved workflow failures",
        "",
    ]
    if unresolved:
        lines.extend(
            [
                "| Status | Workflow | Branch / event | Failed jobs and steps | Latest run |",
                "|---|---|---|---|---|",
            ]
        )
        for entry in unresolved:
            detail = "; ".join(
                f"{item['job']}: {', '.join(item['steps']) or 'job failure'}"
                for item in entry.get("failures", [])
            )
            lines.append(
                f"| {entry['status']} | {markdown_cell(entry['workflow'])} | "
                f"{markdown_cell(entry['branch'])} / {markdown_cell(entry['event'])} | "
                f"{markdown_cell(detail)} | [run]({entry['url']}) |"
            )
    else:
        lines.append("No unresolved workflow failures in the latest completed runs.")

    lines.extend(["", "## Open security and dependency findings", ""])
    for alert in unavailable_alerts:
        if not alert["available"]:
            lines.append(
                f"- **{alert['kind']} status unavailable:** {markdown_cell(alert['detail'])}"
            )
    open_alerts = [item for item in alerts.values() if item["status"] in {"NEW", "PERSISTENT"}]
    if open_alerts:
        for item in sorted(open_alerts, key=lambda entry: (entry["kind"], entry["label"])):
            target = f" ([details]({item['url']}))" if item["url"] else ""
            lines.append(
                f"- **{item['status']} {item['kind']}:** {markdown_cell(item['label'])}{target}"
            )
    elif not unavailable_alerts:
        lines.append("No open findings reported.")

    lines.extend(
        [
            "",
            "## Resolved workflow failures",
            "",
        ]
    )
    if resolved:
        for entry in resolved:
            lines.append(
                f"- **RESOLVED** {markdown_cell(entry['workflow'])} on "
                f"{markdown_cell(entry['branch'])}: [failed run]({entry['url']}), "
                f"verified by [successful run]({entry.get('resolved_url', '')})."
            )
    else:
        lines.append("No previously tracked workflow failures have been resolved yet.")
    resolved_alerts = sorted(
        (item for item in alerts.values() if item["status"] == "RESOLVED"),
        key=lambda item: item.get("resolved_at", ""),
        reverse=True,
    )[:20]
    if resolved_alerts:
        lines.extend(["", "## Resolved security and dependency findings", ""])
        for item in resolved_alerts:
            target = f" ([details]({item['url']}))" if item["url"] else ""
            lines.append(
                f"- **RESOLVED {item['kind']}:** {markdown_cell(item['label'])}{target}"
            )

    lines.extend(
        [
            "",
            "## Policy and retained integrity controls",
            "",
            "- The repository `main` ruleset and its required PR/status checks were removed; no traditional branch protection was configured.",
            "- DCO remains a non-enforced convention. CI results remain visible and are not configured as required checks.",
            "- Source release tests, package validation, and checksums remain fail-closed for the source artifact being published.",
            "- Runtime publication still requires its complete six-target manifest, native validation, hashes, and GitHub attestation because existing consumers verify that update identity. Partial or unauthenticated runtime publication is not safe under the current consumer contract.",
            "- FFmpeg upstream source-signature verification remains as supply-chain validation. Code-signing/notarization of Core artifacts is not required; host applications retain their own distribution policies.",
            "",
        ]
    )
    serialized = json.dumps(
        {"entries": state, "alerts": alerts},
        separators=(",", ":"),
        ensure_ascii=True,
    )
    serialized = serialized.replace("-->", "--\\u003e")
    lines.append(f"<!-- ci-failure-state: {serialized} -->")
    return "\n".join(lines)


def main():
    issue, _ = gh_json([f"repos/{REPOSITORY}/issues/{ISSUE_NUMBER}"])
    previous_state = issue_state(issue.get("body", ""))
    state = previous_state.get("entries", {})
    latest = latest_completed_runs()
    for key, run in latest.items():
        if (
            run["conclusion"] not in SUCCESS_CONCLUSIONS
            and run["conclusion"] not in INCONCLUSIVE_CONCLUSIONS
        ) or run["workflow"] == "source-release":
            run["failures"] = failure_details(run)
    now = datetime.now(timezone.utc).isoformat(timespec="seconds")
    state = merge_entries(state, latest, now)
    alerts = [
        security_alerts("dependabot/alerts", "Dependabot"),
        security_alerts("code-scanning/alerts", "Code scanning"),
        security_alerts("secret-scanning/alerts", "Secret scanning"),
    ]
    alert_state, unavailable_alerts = merge_alerts(
        previous_state.get("alerts", {}), alerts, now
    )
    body = render_body(state, alert_state, unavailable_alerts, now)
    existing_body = re.sub(
        r"Last refreshed: .*", "Last refreshed: <unchanged>", issue.get("body", "")
    )
    new_body = re.sub(r"Last refreshed: .*", "Last refreshed: <unchanged>", body)
    if existing_body != new_body:
        subprocess.run(
            [
                "gh",
                "api",
                "--method",
                "PATCH",
                f"repos/{REPOSITORY}/issues/{ISSUE_NUMBER}",
                "-f",
                f"body={body}",
            ],
            check=True,
            capture_output=True,
            text=True,
        )
    unresolved = sum(entry["status"] in ACTIVE_STATUSES for entry in state.values())
    unresolved += sum(
        entry["status"] in {"NEW", "PERSISTENT"} for entry in alert_state.values()
    )
    print(f"Updated issue #{ISSUE_NUMBER}: {unresolved} unresolved failure(s) or security finding(s).")


if __name__ == "__main__":
    main()
