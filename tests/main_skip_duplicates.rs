use assert_cmd::Command;
use mockito::Server;
use serde_json::Value;
use std::fs;
use tempfile::tempdir;

#[test]
fn main_skips_already_posted() {
    let mut server = Server::new();
    let hh_mock = server
        .mock("GET", "/search/vacancy/rss")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/xml")
        .with_body(
            "<?xml version='1.0' encoding='utf-8'?><rss version=\"2.0\"><channel><item><pubDate>2026-04-20T12:29:41.773+03:00</pubDate><title>Rust dev</title><link>http://example.com/vacancy/1</link></item></channel></rss>",
        )
        .create();

    let tg_mock = server
        .mock("POST", "/bottoken/sendMessage")
        .expect(0)
        .with_status(200)
        .create();

    let dir = tempdir().unwrap();
    let state_path = dir.path().join("state.json");
    fs::write(&state_path, "{\"1\":\"2024-07-08\"}").unwrap();

    Command::cargo_bin("rust-hh-feed")
        .unwrap()
        .env("HH_BASE_URL", server.url())
        .env("TELEGRAM_API_BASE_URL", server.url())
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

    hh_mock.assert();
    tg_mock.assert();
}
