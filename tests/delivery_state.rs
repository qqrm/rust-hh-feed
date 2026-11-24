//! Failure and recovery contracts for the Rust delivery checkpoint.

use assert_cmd::Command;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use mockito::{Matcher, Mock, Server};
use rust_hh_feed::delivery_state::{
    commit, config_from_env, restore, Config, Operation, RestoreOutcome,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

// mockito owns its own runtime, so keep mocks on this thread and only drive
// the library futures through a short-lived runtime.
fn run<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

fn config(server: &Server) -> Config {
    Config::new("test/repo", "TEST", "posted_jobs.json", server.url()).unwrap()
}

fn receipt(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.remote-sha", path.display()))
}

fn write_receipt(path: &Path, sha: &str) {
    fs::write(receipt(path), sha).unwrap();
}

fn envelope(state: &[u8], pending: Option<&str>) -> Value {
    json!({"version": 1, "state": STANDARD.encode(state), "pending": pending})
}

fn remote(envelope: &Value, sha: &str) -> Value {
    json!({"content": STANDARD.encode(envelope.to_string()), "sha": sha})
}

fn hex_digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn contents_get(server: &mut Server, body: Value, status: usize, expect: usize) -> Mock {
    server
        .mock("GET", "/repos/test/repo/contents/posted_jobs.json")
        .match_query(Matcher::Any)
        .with_status(status)
        .with_body(body.to_string())
        .expect(expect)
        .create()
}

fn branch_get(server: &mut Server, exists: bool, expect: usize) -> Mock {
    let (status, body) = if exists {
        (200, json!({"ref": "refs/heads/delivery-state"}))
    } else {
        (404, json!({}))
    };
    server
        .mock("GET", "/repos/test/repo/git/ref/heads/delivery-state")
        .with_status(status)
        .with_body(body.to_string())
        .expect(expect)
        .create()
}

fn repository_get(server: &mut Server, status: usize, expect: usize) -> Mock {
    server
        .mock("GET", "/repos/test/repo")
        .with_status(status)
        .with_body(json!({"default_branch": "main"}).to_string())
        .expect(expect)
        .create()
}

fn head_get(server: &mut Server) -> Mock {
    server
        .mock("GET", "/repos/test/repo/git/ref/heads/main")
        .with_status(200)
        .with_body(json!({"object": {"sha": "head-sha"}}).to_string())
        .create()
}

fn refs_post(server: &mut Server) -> Mock {
    server
        .mock("POST", "/repos/test/repo/git/refs")
        .with_status(201)
        .with_body("{}")
        .create()
}

fn put_payload(next: &Value, sha: Option<&str>) -> Value {
    let mut payload = json!({
        "message": "Checkpoint delivery progress",
        "branch": "delivery-state",
        "content": STANDARD.encode(next.to_string()),
    });
    if let Some(sha) = sha {
        payload["sha"] = json!(sha);
    }
    payload
}

fn contents_put(server: &mut Server, expected: &Value, status: usize) -> Mock {
    server
        .mock("PUT", "/repos/test/repo/contents/posted_jobs.json")
        .match_query(Matcher::Any)
        .match_body(Matcher::Exact(expected.to_string()))
        .with_status(status)
        .with_body(json!({"content": {"sha": "committed-sha"}}).to_string())
        .create()
}

#[test]
fn only_missing_branch_allows_bootstrap() {
    let mut server = Server::new();
    let _contents = contents_get(&mut server, json!({}), 404, 1);
    let _branch = branch_get(&mut server, false, 1);
    let _repository = repository_get(&mut server, 200, 1);
    let dir = tempdir().unwrap();
    let path = dir.path().join("posted_jobs.json");

    assert_eq!(
        run(restore(&config(&server), &path)).unwrap(),
        RestoreOutcome::Bootstrap
    );
    assert!(!path.exists());
    assert_eq!(
        fs::read_to_string(receipt(&path)).unwrap(),
        "confirmed-absent"
    );

    for existing_branch in [true, false] {
        let mut broken = Server::new();
        let _contents = contents_get(&mut broken, json!({}), 404, 1);
        let _branch = branch_get(&mut broken, existing_branch, 1);
        if !existing_branch {
            let _repository = repository_get(&mut broken, 404, 1);
        }
        assert!(run(restore(&config(&broken), &path)).is_err());
        assert!(!path.exists());
    }
}

#[test]
fn auth_and_network_errors_do_not_become_empty_state() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("posted_jobs.json");
    for status in [401, 403] {
        let mut server = Server::new();
        let _contents = contents_get(&mut server, json!({"message": "denied"}), status, 1);
        assert!(run(restore(&config(&server), &path)).is_err());
        assert!(!path.exists());
    }

    let offline = Config::new(
        "test/repo",
        "TEST",
        "posted_jobs.json",
        "http://127.0.0.1:1",
    )
    .unwrap();
    assert!(run(restore(&offline, &path)).is_err());
    assert!(!path.exists());
}

#[test]
fn pending_is_durable_before_send_and_blocks_crash_retry() {
    let identity = "item-2";
    let digest = hex_digest(identity);
    let mut server = Server::new();
    let _contents = contents_get(
        &mut server,
        remote(&envelope(b"confirmed", None), "blob-sha"),
        200,
        2,
    );
    let expected = put_payload(&envelope(b"confirmed", Some(&digest)), Some("blob-sha"));
    let _write = contents_put(&mut server, &expected, 200);
    let dir = tempdir().unwrap();
    let path = dir.path().join("posted_jobs.json");

    run(restore(&config(&server), &path)).unwrap();
    run(commit(
        &config(&server),
        &path,
        Operation::Begin,
        Some(identity),
    ))
    .unwrap();
    assert_eq!(fs::read_to_string(receipt(&path)).unwrap(), "committed-sha");

    for operation in [Operation::Save, Operation::Begin] {
        let mut blocked = Server::new();
        let _contents = contents_get(
            &mut blocked,
            remote(&envelope(b"confirmed", Some(&digest)), "blob-sha"),
            200,
            1,
        );
        assert!(run(commit(&config(&blocked), &path, operation, Some(identity))).is_err());
    }

    let mut blocked = Server::new();
    let _contents = contents_get(
        &mut blocked,
        remote(&envelope(b"confirmed", Some(&digest)), "blob-sha"),
        200,
        1,
    );
    assert!(run(restore(&config(&blocked), &path)).is_err());
}

#[test]
fn ack_promotes_state_and_clears_pending_in_one_write() {
    let mut server = Server::new();
    let _contents = contents_get(
        &mut server,
        remote(&envelope(b"confirmed", Some("pending-hash")), "blob-sha"),
        200,
        1,
    );
    let expected = put_payload(&envelope(b"next confirmed", None), Some("blob-sha"));
    let _write = contents_put(&mut server, &expected, 200);
    let dir = tempdir().unwrap();
    let path = dir.path().join("posted_jobs.json");

    write_receipt(&path, "blob-sha");
    fs::write(&path, b"next confirmed").unwrap();
    run(commit(&config(&server), &path, Operation::Confirm, None)).unwrap();
    assert_eq!(fs::read_to_string(receipt(&path)).unwrap(), "committed-sha");
}

#[test]
fn failed_write_after_ack_leaves_pending_and_blocks_next_run() {
    let mut server = Server::new();
    let _contents = contents_get(
        &mut server,
        remote(&envelope(b"confirmed", Some("pending-hash")), "blob-sha"),
        200,
        1,
    );
    let expected = put_payload(&envelope(b"next confirmed", None), Some("blob-sha"));
    let _write = contents_put(&mut server, &expected, 500);
    let dir = tempdir().unwrap();
    let path = dir.path().join("posted_jobs.json");

    write_receipt(&path, "blob-sha");
    fs::write(&path, b"next confirmed").unwrap();
    assert!(run(commit(&config(&server), &path, Operation::Confirm, None)).is_err());

    fs::remove_file(&path).unwrap();
    let mut next = Server::new();
    let _contents = contents_get(
        &mut next,
        remote(&envelope(b"confirmed", Some("pending-hash")), "blob-sha"),
        200,
        1,
    );
    assert!(run(restore(&config(&next), &path)).is_err());
    assert!(!path.exists());
}

#[test]
fn unchanged_state_is_not_committed_and_valid_state_restores() {
    let mut server = Server::new();
    let _contents = contents_get(
        &mut server,
        remote(&envelope(b"confirmed", None), "blob-sha"),
        200,
        2,
    );
    let dir = tempdir().unwrap();
    let path = dir.path().join("posted_jobs.json");

    run(restore(&config(&server), &path)).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"confirmed");
    run(commit(&config(&server), &path, Operation::Save, None)).unwrap();
}

#[test]
fn corrupt_envelope_is_rejected() {
    for content in ["not valid base64!!!", &STANDARD.encode("{}")] {
        let mut server = Server::new();
        let _contents = contents_get(
            &mut server,
            json!({"content": content, "sha": "blob-sha"}),
            200,
            1,
        );
        let dir = tempdir().unwrap();
        let path = dir.path().join("posted_jobs.json");
        assert!(run(restore(&config(&server), &path)).is_err());
        assert!(!path.exists());
    }
}

#[test]
fn changed_remote_after_restore_never_overwrites_newer_state() {
    for operation in [Operation::Save, Operation::Begin, Operation::Confirm] {
        let mut server = Server::new();
        let _contents = contents_get(
            &mut server,
            remote(&envelope(b"confirmed", None), "blob-sha"),
            200,
            1,
        );
        let dir = tempdir().unwrap();
        let path = dir.path().join("posted_jobs.json");
        run(restore(&config(&server), &path)).unwrap();

        let mut stale = Server::new();
        let _remote = contents_get(
            &mut stale,
            remote(
                &envelope(b"newer confirmed state", Some("another-pending")),
                "newer-sha",
            ),
            200,
            1,
        );
        assert!(run(commit(&config(&stale), &path, operation, Some("item"))).is_err());
    }
}

#[test]
fn missing_restore_receipt_does_not_allow_bootstrap_or_overwrite() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("posted_jobs.json");
    fs::write(&path, b"local stale state").unwrap();

    for exists in [false, true] {
        let mut server = Server::new();
        let body = if exists {
            remote(&envelope(b"remote", None), "blob-sha")
        } else {
            json!({})
        };
        let _contents = contents_get(&mut server, body, if exists { 200 } else { 404 }, 1);
        assert!(run(commit(&config(&server), &path, Operation::Save, None)).is_err());
    }
}

#[test]
fn bootstrap_race_with_another_writer_fails_before_put() {
    let mut bootstrap = Server::new();
    let _contents = contents_get(&mut bootstrap, json!({}), 404, 1);
    let _branch = branch_get(&mut bootstrap, false, 1);
    let _repository = repository_get(&mut bootstrap, 200, 1);
    let dir = tempdir().unwrap();
    let path = dir.path().join("posted_jobs.json");
    assert_eq!(
        run(restore(&config(&bootstrap), &path)).unwrap(),
        RestoreOutcome::Bootstrap
    );
    fs::write(&path, b"initial").unwrap();

    let mut raced = Server::new();
    let _contents = contents_get(
        &mut raced,
        remote(&envelope(b"another runner state", None), "newer-sha"),
        200,
        1,
    );
    assert!(run(commit(&config(&raced), &path, Operation::Save, None)).is_err());
}

#[test]
fn bootstrap_creates_the_delivery_branch_when_initial_state_exists() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("posted_jobs.json");
    fs::write(&path, b"initial").unwrap();
    write_receipt(&path, "confirmed-absent");

    let mut server = Server::new();
    let _contents = contents_get(&mut server, json!({}), 404, 1);
    let _branch = branch_get(&mut server, false, 1);
    let _repository = repository_get(&mut server, 200, 1);
    let _head = head_get(&mut server);
    let _create = refs_post(&mut server);
    let expected = put_payload(&envelope(b"initial", None), None);
    let _write = contents_put(&mut server, &expected, 201);

    run(commit(&config(&server), &path, Operation::Save, None)).unwrap();
    assert_eq!(fs::read_to_string(receipt(&path)).unwrap(), "committed-sha");
}

#[test]
fn cli_restore_signals_bootstrap_with_exit_code_three() {
    let mut server = Server::new();
    let _contents = contents_get(&mut server, json!({}), 404, 1);
    let _branch = branch_get(&mut server, false, 1);
    let _repository = repository_get(&mut server, 200, 1);
    let dir = tempdir().unwrap();
    let path = dir.path().join("posted_jobs.json");

    Command::cargo_bin("rust-hh-feed")
        .unwrap()
        .env("GITHUB_REPOSITORY", "test/repo")
        .env("GH_TOKEN", "TEST")
        .env("DELIVERY_STATE_KEY", "posted_jobs.json")
        .env("GITHUB_API_URL", server.url())
        .args(["checkpoint", "restore", path.to_str().unwrap()])
        .assert()
        .code(3);
}

#[test]
fn checkpoint_configuration_requires_explicit_opt_in() {
    // GITHUB_REPOSITORY is ambient in Actions and must not enable the
    // checkpoint by itself.
    std::env::set_var("GITHUB_REPOSITORY", "test/repo");
    std::env::remove_var("GH_TOKEN");
    std::env::remove_var("DELIVERY_STATE_KEY");
    assert!(config_from_env().unwrap().is_none());

    std::env::set_var("DELIVERY_STATE_KEY", "posted_jobs.json");
    assert!(config_from_env().is_err());
    std::env::remove_var("DELIVERY_STATE_KEY");
    std::env::remove_var("GITHUB_REPOSITORY");
}
