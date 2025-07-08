use mockito::{mock, server_url, Matcher};
use rust_hh_feed::hh::HhClient;

#[tokio::test]
async fn fetch_jobs_parses_mock_response() {
    let body = r#"{"items":[{"id":"1","name":"Rust dev","alternate_url":"http://example.com/1"}]}"#;
    let _m = mock("GET", "/vacancies")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body)
        .create();

    let client = HhClient::with_base_url(server_url());
    let jobs = client.fetch_jobs().await.unwrap();

    assert_eq!(jobs.len(), 1);
    let job = &jobs[0];
    assert_eq!(job.id, "1");
    assert_eq!(job.name, "Rust dev");
    assert_eq!(job.url, "http://example.com/1");
}
