use assert_cmd::Command;
use mockito::{mock, server_url, Matcher};
use std::fs;
use tempfile::tempdir;

#[test]
fn main_does_not_commit_state_after_failed_delivery() {
    let hh_body = r#"{
        "items": [
            {"id":"1","name":"Rust dev","alternate_url":"http://example.com/1"},
            {"id":"2","name":"Rust engineer","alternate_url":"http://example.com/2"}
        ]
    }"#;
    let _hh_mock = mock("GET", "/vacancies")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(hh_body)
        .create();

    let _tg_ok = mock("POST", "/bottoken/sendMessage")
        .match_body(Matcher::Regex("http://example.com/1".into()))
        .with_status(200)
        .create();
    let _tg_fail = mock("POST", "/bottoken/sendMessage")
        .match_body(Matcher::Regex("http://example.com/2".into()))
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
        .env("HH_BASE_URL", server_url())
        .env("TELEGRAM_API_BASE_URL", server_url())
        .env("TELEGRAM_BOT_TOKEN", "token")
        .env("TELEGRAM_CHAT_ID", "1")
        .env("POSTED_JOBS_PATH", &state_path)
        .assert()
        .failure();

    let content = fs::read_to_string(&state_path).unwrap();
    assert_eq!(content, initial_state);

    _hh_mock.assert();
    _tg_ok.assert();
    _tg_fail.assert();
}
