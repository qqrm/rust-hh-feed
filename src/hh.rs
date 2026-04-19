use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use reqwest::header::{HeaderName, USER_AGENT};
use reqwest::{Client, Proxy, Url};
use serde::Deserialize;
use std::collections::HashSet;
use std::time::Duration;

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
    items: Vec<Job>,
    pages: Option<u32>,
}

pub struct HhClient {
    routes: Vec<HhRoute>,
    base_url: String,
    user_agent: String,
}

struct HhRoute {
    client: Client,
    label: String,
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
const DEFAULT_USER_AGENT: &str = "rust-hh-feed/1.0 (qqrm@users.noreply.github.com)";
const USER_AGENT_ENV_VAR: &str = "HH_USER_AGENT";
const HH_USER_AGENT_HEADER: HeaderName = HeaderName::from_static("hh-user-agent");
const PROXY_URLS_ENV_VAR: &str = "HH_PROXY_URLS";
const PROXY_SOURCE_URLS_ENV_VAR: &str = "HH_PROXY_SOURCE_URLS";
const PROXY_PROBE_TIMEOUT_SECS_ENV_VAR: &str = "HH_PROXY_PROBE_TIMEOUT_SECS";
const DEFAULT_PROXY_PROBE_TIMEOUT_SECS: u64 = 5;

fn configured_user_agent() -> String {
    std::env::var(USER_AGENT_ENV_VAR)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_USER_AGENT.to_owned())
}

fn configured_proxy_probe_timeout() -> Result<Duration> {
    let secs = std::env::var(PROXY_PROBE_TIMEOUT_SECS_ENV_VAR)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            value.parse::<u64>().with_context(|| {
                format!(
                    "{PROXY_PROBE_TIMEOUT_SECS_ENV_VAR} must be a positive integer number of seconds"
                )
            })
        })
        .transpose()?
        .unwrap_or(DEFAULT_PROXY_PROBE_TIMEOUT_SECS);

    anyhow::ensure!(
        secs > 0,
        "{PROXY_PROBE_TIMEOUT_SECS_ENV_VAR} must be a positive integer number of seconds"
    );

    Ok(Duration::from_secs(secs))
}

fn split_configured_values(raw: &str) -> impl Iterator<Item = &str> {
    raw.lines()
        .flat_map(|line| line.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| !value.starts_with('#'))
}

fn normalize_proxy_candidate(raw: &str) -> Option<String> {
    let candidate = raw.trim();
    if candidate.is_empty() || candidate.starts_with('#') {
        return None;
    }

    if candidate.contains("://") {
        return Some(candidate.to_owned());
    }

    let parts: Vec<_> = candidate.split(':').collect();
    match parts.as_slice() {
        [host, port] if !host.is_empty() && !port.is_empty() => {
            Some(format!("http://{host}:{port}"))
        }
        [host, port, username, password]
            if !host.is_empty()
                && !port.is_empty()
                && !username.is_empty()
                && !password.is_empty() =>
        {
            Some(format!("http://{username}:{password}@{host}:{port}"))
        }
        _ => None,
    }
}

fn redact_proxy_url(proxy_url: &str) -> String {
    match Url::parse(proxy_url) {
        Ok(url) => {
            let scheme = url.scheme();
            let host = url.host_str().unwrap_or("unknown");
            let port = url
                .port()
                .map(|value| format!(":{value}"))
                .unwrap_or_default();
            format!("{scheme}://{host}{port}")
        }
        Err(_) => proxy_url.to_owned(),
    }
}

async fn fetch_proxy_source_candidates(
    source_client: &Client,
    source_url: &str,
) -> Result<Vec<String>> {
    let response = source_client
        .get(source_url)
        .send()
        .await
        .with_context(|| format!("failed to fetch proxy source {source_url}"))?
        .error_for_status()
        .with_context(|| format!("proxy source returned an unsuccessful status: {source_url}"))?;

    let body = response
        .text()
        .await
        .with_context(|| format!("failed to read proxy source response body from {source_url}"))?;

    Ok(split_configured_values(&body)
        .filter_map(normalize_proxy_candidate)
        .collect())
}

fn build_client_with_timeout(timeout: Duration) -> Result<Client> {
    Client::builder()
        .timeout(timeout)
        .build()
        .context("failed to build HH HTTP client")
}

fn build_proxy_client(proxy_url: &str, timeout: Duration) -> Result<Client> {
    let proxy =
        Proxy::all(proxy_url).with_context(|| format!("failed to configure proxy {proxy_url}"))?;

    Client::builder()
        .proxy(proxy)
        .timeout(timeout)
        .build()
        .with_context(|| format!("failed to build HH client for proxy {proxy_url}"))
}

fn probe_request_url(base_url: &str) -> Url {
    let url = format!("{base_url}/vacancies");
    let mut request_url = Url::parse(&url).expect("HH vacancies URL must be valid");
    request_url
        .query_pairs_mut()
        .append_pair("text", "rust")
        .append_pair("per_page", "1")
        .append_pair("page", "0")
        .append_pair("order_by", "publication_time")
        .append_pair("search_field", "name");
    request_url
}

async fn probe_vacancies_access(client: &Client, base_url: &str, user_agent: &str) -> bool {
    let request_url = probe_request_url(base_url);
    match client
        .get(request_url)
        .header(USER_AGENT, user_agent)
        .header(HH_USER_AGENT_HEADER.clone(), user_agent)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => true,
        Ok(response) => {
            log::debug!("HH proxy probe received status {}", response.status());
            false
        }
        Err(error) => {
            log::debug!("HH proxy probe failed: {error}");
            false
        }
    }
}

async fn configured_proxy_candidates(timeout: Duration) -> Result<Vec<String>> {
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();

    if let Ok(raw) = std::env::var(PROXY_URLS_ENV_VAR) {
        for candidate in split_configured_values(&raw).filter_map(normalize_proxy_candidate) {
            if seen.insert(candidate.clone()) {
                candidates.push(candidate);
            }
        }
    }

    if let Ok(raw_sources) = std::env::var(PROXY_SOURCE_URLS_ENV_VAR) {
        let source_client = build_client_with_timeout(timeout)?;
        for source_url in split_configured_values(&raw_sources) {
            let source_candidates =
                fetch_proxy_source_candidates(&source_client, source_url).await?;
            for candidate in source_candidates {
                if seen.insert(candidate.clone()) {
                    candidates.push(candidate);
                }
            }
        }
    }

    Ok(candidates)
}

impl HhClient {
    pub fn new() -> Self {
        Self {
            routes: vec![HhRoute {
                client: Client::new(),
                label: "direct".to_owned(),
            }],
            base_url: "https://api.hh.ru".into(),
            user_agent: configured_user_agent(),
        }
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            routes: vec![HhRoute {
                client: Client::new(),
                label: "direct".to_owned(),
            }],
            base_url: base_url.into(),
            user_agent: configured_user_agent(),
        }
    }

    pub async fn from_env() -> Result<Self> {
        Self::from_env_with_base_url("https://api.hh.ru").await
    }

    pub async fn from_env_with_base_url(base_url: impl Into<String>) -> Result<Self> {
        let base_url = base_url.into();
        let user_agent = configured_user_agent();
        let timeout = configured_proxy_probe_timeout()?;
        let proxy_candidates = configured_proxy_candidates(timeout).await?;
        let direct_client = build_client_with_timeout(timeout)?;

        if proxy_candidates.is_empty() {
            return Ok(Self {
                routes: vec![HhRoute {
                    client: direct_client,
                    label: "direct".to_owned(),
                }],
                base_url,
                user_agent,
            });
        }

        log::info!(
            "Loaded {} proxy candidate(s) for HeadHunter requests",
            proxy_candidates.len()
        );

        let mut routes = Vec::new();
        for candidate in proxy_candidates {
            let redacted = redact_proxy_url(&candidate);
            let client = match build_proxy_client(&candidate, timeout) {
                Ok(client) => client,
                Err(error) => {
                    log::warn!("Skipping unusable proxy {redacted}: {error:#}");
                    continue;
                }
            };

            if probe_vacancies_access(&client, &base_url, &user_agent).await {
                log::info!("Accepted HeadHunter proxy candidate {redacted}");
                routes.push(HhRoute {
                    client,
                    label: redacted,
                });
                continue;
            }

            log::warn!("Proxy {redacted} did not pass the HeadHunter vacancies probe");
        }

        if routes.is_empty() {
            log::warn!(
                "No configured proxy passed the HeadHunter vacancies probe, falling back to direct access"
            );
        }

        routes.push(HhRoute {
            client: direct_client,
            label: "direct".to_owned(),
        });

        Ok(Self {
            routes,
            base_url,
            user_agent,
        })
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
        let mut last_error = None;

        for route in &self.routes {
            log::info!("Attempting HeadHunter fetch via {}", route.label);
            match self.fetch_jobs_between_via(&route.client, from, to).await {
                Ok(jobs) => {
                    if route.label != "direct" {
                        log::info!("HeadHunter fetch succeeded via {}", route.label);
                    }
                    return Ok(jobs);
                }
                Err(error) => {
                    log::warn!("HeadHunter fetch failed via {}: {}", route.label, error);
                    last_error = Some(error);
                }
            }
        }

        Err(last_error.expect("HH client must have at least one route"))
    }

    async fn fetch_jobs_between_via(
        &self,
        client: &Client,
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
                .append_pair("search_field", "name")
                .append_pair(
                    "date_from",
                    &from.to_rfc3339_opts(SecondsFormat::Secs, true),
                )
                .append_pair("date_to", &to.to_rfc3339_opts(SecondsFormat::Secs, true));

            let response = client
                .get(request_url)
                .header(USER_AGENT, &self.user_agent)
                .header(HH_USER_AGENT_HEADER.clone(), &self.user_agent)
                .send()
                .await?
                .error_for_status()?
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

#[cfg(test)]
mod tests {
    use super::{normalize_proxy_candidate, redact_proxy_url, split_configured_values};

    #[test]
    fn split_configured_values_supports_lines_and_commas() {
        let values: Vec<_> = split_configured_values("one, two\nthree\n# four\n\nfive")
            .map(ToOwned::to_owned)
            .collect();

        assert_eq!(values, vec!["one", "two", "three", "five"]);
    }

    #[test]
    fn normalize_proxy_candidate_supports_common_formats() {
        assert_eq!(
            normalize_proxy_candidate("10.0.0.1:8080").as_deref(),
            Some("http://10.0.0.1:8080")
        );
        assert_eq!(
            normalize_proxy_candidate("socks5://10.0.0.1:1080").as_deref(),
            Some("socks5://10.0.0.1:1080")
        );
        assert_eq!(
            normalize_proxy_candidate("10.0.0.1:8080:user:pass").as_deref(),
            Some("http://user:pass@10.0.0.1:8080")
        );
        assert!(normalize_proxy_candidate("invalid").is_none());
    }

    #[test]
    fn redact_proxy_url_hides_credentials() {
        assert_eq!(
            redact_proxy_url("http://user:pass@10.0.0.1:8080"),
            "http://10.0.0.1:8080"
        );
    }
}
