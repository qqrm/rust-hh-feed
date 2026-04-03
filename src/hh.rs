use chrono::{DateTime, SecondsFormat, Utc};
use reqwest::Url;
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

#[derive(Debug, Deserialize)]
struct VacanciesResponse {
    #[serde(default)]
    items: Vec<Job>,
    pages: Option<u32>,
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
        let to = Utc::now();
        let from = to - chrono::Duration::minutes(45);
        self.fetch_jobs_between(from, to).await
    }

    pub async fn fetch_jobs_between(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<Job>, reqwest::Error> {
        let url = format!("{}/vacancies", self.base_url);
        log::debug!("Requesting jobs from {url}");
        let search_query = SEARCH_TERMS.join(" OR ");
        let mut page = 0;
        let mut jobs = Vec::new();

        loop {
            let mut request_url = Url::parse(&url).expect("HH vacancies URL must be valid");
            request_url
                .query_pairs_mut()
                .append_pair("text", &search_query)
                .append_pair("per_page", "100")
                .append_pair("page", &page.to_string())
                .append_pair("order_by", "publication_time")
                .append_pair(
                    "date_from",
                    &from.to_rfc3339_opts(SecondsFormat::Secs, true),
                )
                .append_pair("date_to", &to.to_rfc3339_opts(SecondsFormat::Secs, true));

            let response = self
                .client
                .get(request_url)
                .header(
                    "User-Agent",
                    "Mozilla/5.0 (compatible; rust-bot/1.0; +https://github.com/qqrm/rust-hh-feed)",
                )
                .send()
                .await?
                .json::<VacanciesResponse>()
                .await?;

            log::debug!(
                "Fetched page {} with {} item(s) for range {}..{}",
                page,
                response.items.len(),
                from,
                to
            );

            let pages = response
                .pages
                .unwrap_or(u32::from(response.items.is_empty()));
            jobs.extend(response.items);

            page += 1;
            if page >= pages.max(1) {
                break;
            }
        }

        log::debug!("Parsed {} jobs", jobs.len());
        Ok(jobs)
    }
}

impl Default for HhClient {
    fn default() -> Self {
        Self::new()
    }
}
