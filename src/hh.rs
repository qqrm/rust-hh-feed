use chrono::{Duration, SecondsFormat, Utc};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Snippet {
    pub requirement: Option<String>,
    pub responsibility: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Job {
    pub id: String,
    pub name: String,
    #[serde(rename = "alternate_url")]
    pub url: String,
    pub snippet: Option<Snippet>,
}

pub struct HhClient {
    client: reqwest::Client,
    base_url: String,
}

/// List of lower-case search terms used to query HeadHunter.
/// Variations cover common spellings of Rust-related job titles.
const SEARCH_TERMS: &[&str] = &[
    "rust",
    "rust-разработчик",
    "rust-developer",
    "rust-programmer",
    "rust-программист",
];

impl HhClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: "https://api.hh.ru".into(),
        }
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into(),
        }
    }

    pub async fn fetch_jobs(&self) -> Result<Vec<Job>, reqwest::Error> {
        let url = format!("{}/vacancies", self.base_url);
        let to = Utc::now();
        // Search the last 90 minutes to avoid missing jobs when the pipeline is slow.
        let from = to - Duration::minutes(90);
        log::debug!("Requesting jobs from {url}");
        let search_query = SEARCH_TERMS.join(" OR ");
        let resp = self
            .client
            .get(&url)
            .query(&[
                ("text", search_query.as_str()),
                ("per_page", "100"),
                ("order_by", "publication_time"),
                (
                    "date_from",
                    &from.to_rfc3339_opts(SecondsFormat::Secs, true),
                ),
                ("date_to", &to.to_rfc3339_opts(SecondsFormat::Secs, true)),
            ])
            .header(
                "User-Agent",
                "Mozilla/5.0 (compatible; rust-bot/1.0; +https://github.com/qqrm/rust-hh-feed)",
            )
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        log::debug!("Raw response: {resp}");

        let items = resp.get("items");
        if let Some(array) = items.and_then(|v| v.as_array()) {
            log::debug!("Found {} items in response", array.len());
        } else {
            log::debug!("No items field found in response");
        }

        let jobs = items
            .and_then(|v| serde_json::from_value::<Vec<Job>>(v.clone()).ok())
            .unwrap_or_default();
        log::debug!("Parsed {} jobs", jobs.len());
        Ok(jobs)
    }
}

impl Default for HhClient {
    fn default() -> Self {
        Self::new()
    }
}
