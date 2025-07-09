use chrono::{Duration, Utc};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Job {
    pub id: String,
    pub name: String,
    #[serde(rename = "alternate_url")]
    pub url: String,
}

pub struct HhClient {
    client: reqwest::Client,
    base_url: String,
}

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
        // Narrow the search window to a single hour because the bot runs every hour.
        let from = to - Duration::hours(1);
        log::debug!("Requesting jobs from {url}");
        let resp = self
            .client
            .get(&url)
            .query(&[
                ("text", "Rust"),
                ("search_field", "name"),
                ("per_page", "100"),
                ("order_by", "publication_time"),
                ("date_from", &from.format("%Y-%m-%dT%H:%M:%S").to_string()),
                ("date_to", &to.format("%Y-%m-%dT%H:%M:%S").to_string()),
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
