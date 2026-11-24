//! Provenance checks and their end-to-end CLI wrapper.

use assert_cmd::Command;
use mockito::{Matcher, Server};
use rust_hh_feed::policy::verified_bot_commits;
use serde_json::{json, Value};
use std::fs;
use tempfile::tempdir;

fn commit(sha: &str, author: &str, verified: bool) -> Value {
    json!({
        "sha": sha,
        "author": {"login": author},
        "commit": {"verification": {"verified": verified}},
    })
}

#[test]
fn signed_bot_batch_needs_no_version_metadata_or_review() {
    let commits = vec![
        commit("first", "dependabot[bot]", true),
        commit("head", "dependabot[bot]", true),
    ];
    assert!(verified_bot_commits(&commits, "head"));
}

#[test]
fn empty_batch_fails_closed() {
    assert!(!verified_bot_commits(&[], "head"));
}

#[test]
fn changed_head_fails_closed() {
    assert!(!verified_bot_commits(
        &[commit("new", "dependabot[bot]", true)],
        "head"
    ));
}

#[test]
fn every_commit_must_be_from_dependabot() {
    for author in ["contributor", "github-actions[bot]", "dependabot"] {
        let commits = vec![
            commit("first", author, true),
            commit("head", "dependabot[bot]", true),
        ];
        assert!(!verified_bot_commits(&commits, "head"));
    }
}

#[test]
fn every_commit_must_have_a_verified_signature() {
    let unsigned = vec![
        commit("first", "dependabot[bot]", false),
        commit("head", "dependabot[bot]", true),
    ];
    assert!(!verified_bot_commits(&unsigned, "head"));
    let incomplete = vec![
        json!({"sha": "first"}),
        commit("head", "dependabot[bot]", true),
    ];
    assert!(!verified_bot_commits(&incomplete, "head"));
    let null_author = vec![
        json!({"sha": "first", "author": null, "commit": {"verification": {"verified": true}}}),
        commit("head", "dependabot[bot]", true),
    ];
    assert!(!verified_bot_commits(&null_author, "head"));
}

#[test]
fn cli_verifies_paginated_signed_commits() {
    let mut server = Server::new();
    let first_page: Vec<Value> = (0..100)
        .map(|index| commit(&format!("c{index}"), "dependabot[bot]", true))
        .collect();
    let last = commit("head", "dependabot[bot]", true);
    let _page_one = server
        .mock("GET", "/repos/test/repo/pulls/7/commits")
        .match_query(Matcher::Regex("page=1".into()))
        .with_status(200)
        .with_body(json!(first_page).to_string())
        .create();
    let _page_two = server
        .mock("GET", "/repos/test/repo/pulls/7/commits")
        .match_query(Matcher::Regex("page=2".into()))
        .with_status(200)
        .with_body(json!([last]).to_string())
        .create();

    let dir = tempdir().unwrap();
    let event = dir.path().join("event.json");
    fs::write(
        &event,
        json!({
            "pull_request": {"number": 7, "head": {"sha": "head"}},
            "repository": {"full_name": "test/repo"},
        })
        .to_string(),
    )
    .unwrap();

    Command::cargo_bin("rust-hh-feed")
        .unwrap()
        .env("GITHUB_API_URL", server.url())
        .env("GITHUB_EVENT_PATH", event.to_str().unwrap())
        .env("GH_TOKEN", "TEST")
        .args(["dependency-policy", "verify"])
        .assert()
        .success();

    _page_one.assert();
    _page_two.assert();
}

#[test]
fn cli_rejects_an_unsigned_batch() {
    let mut server = Server::new();
    let unsigned = commit("head", "contributor", true);
    let _page = server
        .mock("GET", "/repos/test/repo/pulls/7/commits")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_body(json!([unsigned]).to_string())
        .create();

    let dir = tempdir().unwrap();
    let event = dir.path().join("event.json");
    fs::write(
        &event,
        json!({
            "pull_request": {"number": 7, "head": {"sha": "head"}},
            "repository": {"full_name": "test/repo"},
        })
        .to_string(),
    )
    .unwrap();

    Command::cargo_bin("rust-hh-feed")
        .unwrap()
        .env("GITHUB_API_URL", server.url())
        .env("GITHUB_EVENT_PATH", event.to_str().unwrap())
        .env("GH_TOKEN", "TEST")
        .args(["dependency-policy", "verify"])
        .assert()
        .failure();
}
