use chrono::{SecondsFormat, TimeZone, Utc};
use mockito::{Matcher, Server};
use rust_hh_feed::hh::HhClient;

#[tokio::test]
async fn fetch_jobs_parses_mock_response() {
    let mut server = Server::new_async().await;
    let body = r#"{"items":[{"id":"1","name":"Rust dev","alternate_url":"http://example.com/1","snippet":{"requirement":"Rust experience"}}]}"#;
    let mock = server
        .mock("GET", "/vacancies")
        .match_query(Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(body)
        .create_async()
        .await;

    let client = HhClient::with_base_url(server.url());
    let jobs = client.fetch_jobs().await.unwrap();

    assert_eq!(jobs.len(), 1);
    let job = &jobs[0];
    assert_eq!(job.id, "1");
    assert_eq!(job.name, "Rust dev");
    assert_eq!(job.url, "http://example.com/1");
    assert_eq!(
        job.snippet.as_ref().and_then(|s| s.requirement.as_deref()),
        Some("Rust experience"),
    );

    mock.assert_async().await;
}

#[tokio::test]
async fn fetch_jobs_between_requests_all_pages_for_range() {
    let mut server = Server::new_async().await;
    let from = Utc.with_ymd_and_hms(2026, 4, 3, 7, 15, 0).unwrap();
    let to = Utc.with_ymd_and_hms(2026, 4, 3, 8, 0, 0).unwrap();
    let from_rfc3339 = from.to_rfc3339_opts(SecondsFormat::Secs, true);
    let to_rfc3339 = to.to_rfc3339_opts(SecondsFormat::Secs, true);

    let first_page = server
        .mock("GET", "/vacancies")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("page".into(), "0".into()),
            Matcher::UrlEncoded("per_page".into(), "100".into()),
            Matcher::UrlEncoded("date_from".into(), from_rfc3339.clone()),
            Matcher::UrlEncoded("date_to".into(), to_rfc3339.clone()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
                "pages": 2,
                "items": [
                    {"id":"1","name":"Rust dev","alternate_url":"http://example.com/1"}
                ]
            }"#,
        )
        .create_async()
        .await;

    let second_page = server
        .mock("GET", "/vacancies")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("page".into(), "1".into()),
            Matcher::UrlEncoded("per_page".into(), "100".into()),
            Matcher::UrlEncoded("date_from".into(), from_rfc3339),
            Matcher::UrlEncoded("date_to".into(), to_rfc3339),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
                "pages": 2,
                "items": [
                    {"id":"2","name":"Rust engineer","alternate_url":"http://example.com/2"}
                ]
            }"#,
        )
        .create_async()
        .await;

    let client = HhClient::with_base_url(server.url());
    let jobs = client.fetch_jobs_between(from, to).await.unwrap();

    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs[0].id, "1");
    assert_eq!(jobs[1].id, "2");

    first_page.assert_async().await;
    second_page.assert_async().await;
}
