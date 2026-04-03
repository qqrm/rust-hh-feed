use assert_cmd::Command;
use mockito::{mock, server_url};
use serde_json::Value;
use std::fs;
use tempfile::tempdir;

#[test]
fn main_skips_already_posted() {
    let _hh_mock = mock("GET", "/vacancies")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body("{\"items\":[{\"id\":\"1\",\"name\":\"Rust dev\",\"alternate_url\":\"http://example.com/1\"}]}")
        .create();

    let _tg_mock = mock("POST", "/bottoken/sendMessage")
        .expect(0)
        .with_status(200)
        .create();

    let dir = tempdir().unwrap();
    let state_path = dir.path().join("state.json");
    fs::write(&state_path, "{\"1\":\"2024-07-08\"}").unwrap();

    Command::cargo_bin("rust-hh-feed")
        .unwrap()
        .env("HH_BASE_URL", server_url())
        .env("TELEGRAM_API_BASE_URL", server_url())
        .env("TELEGRAM_BOT_TOKEN", "token")
        .env("TELEGRAM_CHAT_ID", "1")
        .env("JOB_RETENTION_DAYS", "1000")
        .env("POSTED_JOBS_PATH", &state_path)
        .assert()
        .success();

    let state: Value = serde_json::from_str(&fs::read_to_string(&state_path).unwrap()).unwrap();
    assert_eq!(state["version"], 2);
    assert_eq!(state["jobs"]["1"], "2024-07-08");
    assert!(state["last_successful_run_at"].as_str().is_some());

    _hh_mock.assert();
    _tg_mock.assert();
}
