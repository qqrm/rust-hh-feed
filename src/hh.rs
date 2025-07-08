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
        let resp = self
            .client
            .get(format!("{}/vacancies", self.base_url))
            .query(&[
                ("text", "Rust"),
                ("search_field", "name"),
                ("per_page", "20"),
            ])
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        let jobs = resp
            .get("items")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        Ok(jobs)
    }
}

impl Default for HhClient {
    fn default() -> Self {
        Self::new()
    }
}
