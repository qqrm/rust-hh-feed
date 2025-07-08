use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Job {
    pub id: String,
    pub name: String,
    #[serde(rename = "alternate_url")]
    pub url: String,
}

pub struct HhClient {
    client: reqwest::Client,
}

impl HhClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    pub async fn fetch_jobs(&self) -> Result<Vec<Job>, reqwest::Error> {
        let resp = self
            .client
            .get("https://api.hh.ru/vacancies")
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
