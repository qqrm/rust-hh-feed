use assert_cmd::Command;
use mockito::{mock, server_url};
use std::fs;
use tempfile::tempdir;

#[test]
fn main_posts_jobs_with_rust_in_description() {
    let hh_body = r#"{"items":[{"id":"1","name":"Backend dev","alternate_url":"http://example.com/1","snippet":{"requirement":"Proficiency in Rust"}}]}"#;
    let _hh_mock = mock("GET", "/vacancies")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(hh_body)
        .create();

    let _tg_mock = mock("POST", "/bottoken/sendMessage")
        .expect(1)
        .with_status(200)
        .create();

    let dir = tempdir().unwrap();
    let state_path = dir.path().join("state.json");

    Command::cargo_bin("rust-hh-feed")
        .unwrap()
        .env("HH_BASE_URL", server_url())
        .env("TELEGRAM_API_BASE_URL", server_url())
        .env("TELEGRAM_BOT_TOKEN", "token")
        .env("TELEGRAM_CHAT_ID", "1")
        .env("POSTED_JOBS_PATH", &state_path)
        .assert()
        .success();

    let content = fs::read_to_string(&state_path).unwrap();
    assert!(content.contains("\"1\""));

    _hh_mock.assert();
    _tg_mock.assert();
}
