mod common;

use assert_cmd::Command;
use mockito::{Matcher, Server};
use std::fs;
use tempfile::tempdir;

#[test]
fn main_does_not_commit_state_after_failed_delivery() {
    let mut server = Server::new();
    let first_pub_date = common::recent_rfc3339(5);
    let second_pub_date = common::recent_rfc3339(65);
    let hh_body = common::rss_feed(&[
        (&first_pub_date, "Rust dev", "http://example.com/vacancy/1"),
        (
            &second_pub_date,
            "Rust engineer",
            "http://example.com/vacancy/2",
        ),
    ]);
    let hh_mock = server
        .mock("GET", "/search/vacancy/rss")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/xml")
        .with_body(hh_body)
        .create();

    let tg_ok = server
        .mock("POST", "/bottoken/sendMessage")
        .match_body(Matcher::Regex("http://example.com/vacancy/1".into()))
        .with_status(200)
        .create();
    let tg_fail = server
        .mock("POST", "/bottoken/sendMessage")
        .match_body(Matcher::Regex("http://example.com/vacancy/2".into()))
        .with_status(500)
        .create();

    let dir = tempdir().unwrap();
    let state_path = dir.path().join("state.json");
    let initial_state = r#"{
  "version": 2,
  "last_successful_run_at": "2026-04-03T00:00:00Z",
  "jobs": {
    "existing": "2026-04-03"
  }
}"#;
    fs::write(&state_path, initial_state).unwrap();

    Command::cargo_bin("rust-hh-feed")
        .unwrap()
        .env("HH_BASE_URL", server.url())
        .env("TELEGRAM_API_BASE_URL", server.url())
        .env("TELEGRAM_BOT_TOKEN", "token")
        .env("TELEGRAM_CHAT_ID", "1")
        .env("POSTED_JOBS_PATH", &state_path)
        .assert()
        .failure();

    let content = fs::read_to_string(&state_path).unwrap();
    assert_eq!(content, initial_state);

    hh_mock.assert();
    tg_ok.assert();
    tg_fail.assert();
}
