//! Exercise the actual jq programs used by the metadata-only write controller.

use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn evaluate(policy: &str, value: &Value, args: &[(&str, &str)]) -> bool {
    let workflow = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/dependabot-automerge.yml"),
    )
    .unwrap();
    let marker = format!("# {policy}-policy-begin");
    let after_begin = workflow
        .split_once(&marker)
        .unwrap_or_else(|| panic!("missing {policy} policy begin marker"))
        .1;
    let program = after_begin
        .split_once(&format!("# {policy}-policy-end"))
        .unwrap_or_else(|| panic!("missing {policy} policy end marker"))
        .0
        .trim();

    let mut command = Command::new("jq");
    command.arg("-e");
    for (name, argument) in args {
        command.arg("--arg").arg(name).arg(argument);
    }
    command
        .arg(program)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command.spawn().expect("jq must be available in CI");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(value.to_string().as_bytes())
        .unwrap();
    child.wait().unwrap().success()
}

fn ready_pr() -> Value {
    let checks: Vec<Value> = ["ci-gate", "workflow-security-gate", "Analyze (rust)"]
        .iter()
        .map(|name| {
            json!({
                "__typename": "CheckRun",
                "name": name,
                "status": "COMPLETED",
                "conclusion": "SUCCESS",
            })
        })
        .collect();
    json!({
        "author": {"login": "app/dependabot"},
        "isCrossRepository": false,
        "headRefOid": "checked",
        "baseRefName": "main",
        "isDraft": false,
        "state": "OPEN",
        "mergeStateStatus": "CLEAN",
        "statusCheckRollup": checks,
    })
}

#[test]
fn checked_bot_needs_no_human_review() {
    assert!(evaluate("eligibility", &ready_pr(), &[("sha", "checked")]));
}

#[test]
fn merged_checked_head_can_recover_failed_dispatch() {
    let mut pr = ready_pr();
    pr["state"] = json!("MERGED");
    pr["mergeCommit"] = json!({"oid": "a".repeat(40)});
    pr["mergeStateStatus"] = json!("UNKNOWN");
    assert!(evaluate("eligibility", &pr, &[("sha", "checked")]));
    pr["headRefOid"] = json!("other");
    assert!(!evaluate("eligibility", &pr, &[("sha", "checked")]));
}

#[test]
fn wrong_identity_or_target_is_rejected() {
    let cases = [
        ("author", json!({"login": "qqrm"})),
        ("isCrossRepository", json!(true)),
        ("isDraft", json!(true)),
        ("baseRefName", json!("other")),
        ("state", json!("CLOSED")),
        ("headRefOid", json!("replaced")),
        ("mergeStateStatus", json!("UNKNOWN")),
        ("mergeStateStatus", json!("BEHIND")),
    ];
    for (key, value) in cases {
        let mut pr = ready_pr();
        pr[key] = value;
        assert!(!evaluate("eligibility", &pr, &[("sha", "checked")]));
    }
}

#[test]
fn missing_failed_skipped_or_pending_required_gate_is_rejected() {
    for name in ["ci-gate", "workflow-security-gate"] {
        for conclusion in ["FAILURE", "CANCELLED", "SKIPPED", "NEUTRAL", ""] {
            let mut pr = ready_pr();
            let checks = pr["statusCheckRollup"].as_array_mut().unwrap();
            let check = checks.iter_mut().find(|c| c["name"] == name).unwrap();
            check["conclusion"] = json!(conclusion);
            assert!(!evaluate("eligibility", &pr, &[("sha", "checked")]));
        }
        let mut pr = ready_pr();
        pr["statusCheckRollup"] = json!(pr["statusCheckRollup"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["name"] != name)
            .collect::<Vec<_>>());
        assert!(!evaluate("eligibility", &pr, &[("sha", "checked")]));
    }
}

#[test]
fn other_analysis_must_finish_without_failure() {
    for (status, conclusion) in [
        ("IN_PROGRESS", ""),
        ("QUEUED", ""),
        ("COMPLETED", "FAILURE"),
        ("COMPLETED", "TIMED_OUT"),
    ] {
        let mut pr = ready_pr();
        let check = pr["statusCheckRollup"]
            .as_array_mut()
            .unwrap()
            .last_mut()
            .unwrap();
        check["status"] = json!(status);
        check["conclusion"] = json!(conclusion);
        assert!(!evaluate("eligibility", &pr, &[("sha", "checked")]));
    }
}

#[test]
fn legacy_status_context_must_succeed() {
    for state in ["SUCCESS", "PENDING", "FAILURE", "ERROR"] {
        let mut pr = ready_pr();
        pr["statusCheckRollup"]
            .as_array_mut()
            .unwrap()
            .push(json!({"__typename": "StatusContext", "state": state}));
        assert_eq!(
            evaluate("eligibility", &pr, &[("sha", "checked")]),
            state == "SUCCESS"
        );
    }
}

#[test]
fn unknown_check_type_fails_closed() {
    let mut pr = ready_pr();
    pr["statusCheckRollup"]
        .as_array_mut()
        .unwrap()
        .push(json!({"__typename": "Unknown", "state": "SUCCESS"}));
    assert!(!evaluate("eligibility", &pr, &[("sha", "checked")]));
}

#[test]
fn paginated_signed_provenance() {
    let first = json!({
        "sha": "first",
        "author": {"login": "dependabot[bot]"},
        "committer": {"login": "web-flow"},
        "commit": {"verification": {"verified": true}},
    });
    let mut last = first.clone();
    last["sha"] = json!("checked");
    assert!(evaluate(
        "provenance",
        &json!([vec![first.clone()], vec![last.clone()]]),
        &[("sha", "checked")]
    ));

    for (key, value) in [
        ("author", json!({"login": "qqrm"})),
        ("committer", json!({"login": "qqrm"})),
        ("commit", json!({"verification": {"verified": false}})),
    ] {
        let mut invalid = first.clone();
        invalid[key] = value;
        assert!(!evaluate(
            "provenance",
            &json!([vec![invalid], vec![last.clone()]]),
            &[("sha", "checked")]
        ));
    }
    assert!(!evaluate(
        "provenance",
        &json!([vec![first]]),
        &[("sha", "checked")]
    ));
    assert!(!evaluate("provenance", &json!([[]]), &[("sha", "checked")]));
}

#[test]
fn current_main_must_be_an_ancestor_of_batch() {
    assert!(evaluate(
        "freshness",
        &json!({"behind_by": 0, "merge_base_commit": {"sha": "main"}}),
        &[("base", "main")]
    ));
    for response in [
        json!({}),
        json!({"behind_by": 1, "merge_base_commit": {"sha": "main"}}),
        json!({"behind_by": 0, "merge_base_commit": {"sha": "old"}}),
    ] {
        assert!(!evaluate("freshness", &response, &[("base", "main")]));
    }
}
