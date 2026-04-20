use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use quick_xml::de::from_str;
use reqwest::header::USER_AGENT;
use reqwest::{Client, Proxy, Url};
use serde::Deserialize;
use std::borrow::Cow;
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
struct SearchRssFeed {
    channel: SearchRssChannel,
}

#[derive(Debug, Deserialize)]
struct SearchRssChannel {
    #[serde(default)]
    item: Vec<SearchRssItem>,
}

#[derive(Debug, Deserialize)]
struct SearchRssItem {
    title: String,
    link: String,
    #[serde(rename = "pubDate")]
    pub_date: String,
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

const SEARCH_QUERY: &str = "rust";
const DEFAULT_USER_AGENT: &str = "rust-hh-feed/1.0 (qqrm@users.noreply.github.com)";
const USER_AGENT_ENV_VAR: &str = "HH_USER_AGENT";
const PROXY_URLS_ENV_VAR: &str = "HH_PROXY_URLS";
const PROXY_SOURCE_URLS_ENV_VAR: &str = "HH_PROXY_SOURCE_URLS";
const PROXY_PROBE_TIMEOUT_SECS_ENV_VAR: &str = "HH_PROXY_PROBE_TIMEOUT_SECS";
const DEFAULT_PROXY_PROBE_TIMEOUT_SECS: u64 = 5;
const MAX_PROXY_CANDIDATES: usize = 64;
const MAX_PROXY_CANDIDATES_PER_SOURCE: usize = 24;
const MAX_WORKING_PROXY_ROUTES: usize = 1;

#[derive(Clone, Copy)]
enum ProxySourceFormat {
    PlainText,
    SpysPlainText {
        default_scheme: &'static str,
        country_code: &'static str,
    },
    OpenProxyListText {
        default_scheme: &'static str,
        country_code: &'static str,
    },
    FreeProxy24Json {
        country_code: &'static str,
    },
}

#[derive(Clone)]
struct ProxySource {
    url: Cow<'static, str>,
    format: ProxySourceFormat,
}

const DEFAULT_PROXY_SOURCES: &[ProxySource] = &[
    ProxySource {
        url: Cow::Borrowed(
            "https://freeproxy24.com/api/free-proxy-list?limit=100&page=1&country=RU&sortBy=lastChecked&sortType=desc",
        ),
        format: ProxySourceFormat::FreeProxy24Json { country_code: "RU" },
    },
    ProxySource {
        url: Cow::Borrowed("https://raw.githubusercontent.com/roosterkid/openproxylist/main/HTTPS.txt"),
        format: ProxySourceFormat::OpenProxyListText {
            default_scheme: "http",
            country_code: "RU",
        },
    },
    ProxySource {
        url: Cow::Borrowed("https://raw.githubusercontent.com/roosterkid/openproxylist/main/SOCKS4.txt"),
        format: ProxySourceFormat::OpenProxyListText {
            default_scheme: "socks4",
            country_code: "RU",
        },
    },
    ProxySource {
        url: Cow::Borrowed("https://raw.githubusercontent.com/roosterkid/openproxylist/main/SOCKS5.txt"),
        format: ProxySourceFormat::OpenProxyListText {
            default_scheme: "socks5",
            country_code: "RU",
        },
    },
    ProxySource {
        url: Cow::Borrowed("https://spys.me/proxy.txt"),
        format: ProxySourceFormat::SpysPlainText {
            default_scheme: "http",
            country_code: "RU",
        },
    },
    ProxySource {
        url: Cow::Borrowed("https://spys.me/socks.txt"),
        format: ProxySourceFormat::SpysPlainText {
            default_scheme: "socks5",
            country_code: "RU",
        },
    },
];

#[derive(Debug, Deserialize)]
struct FreeProxy24Entry {
    ip: String,
    port: String,
    country: String,
    protocols: Vec<String>,
}

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

fn normalize_proxy_candidate_with_scheme(raw: &str, default_scheme: &str) -> Option<String> {
    let candidate = raw.trim();
    if candidate.is_empty() || candidate.starts_with('#') {
        return None;
    }

    if candidate.contains("://") {
        return normalize_proxy_candidate(candidate);
    }

    let parts: Vec<_> = candidate.split(':').collect();
    match parts.as_slice() {
        [host, port] if !host.is_empty() && !port.is_empty() => {
            Some(format!("{default_scheme}://{host}:{port}"))
        }
        [host, port, username, password]
            if !host.is_empty()
                && !port.is_empty()
                && !username.is_empty()
                && !password.is_empty() =>
        {
            Some(format!(
                "{default_scheme}://{username}:{password}@{host}:{port}"
            ))
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

fn parse_spys_proxy_candidates(
    body: &str,
    default_scheme: &str,
    country_code: &str,
) -> Vec<String> {
    body.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed.starts_with('#')
                || !trimmed
                    .chars()
                    .next()
                    .is_some_and(|value| value.is_ascii_digit())
            {
                return None;
            }

            let mut tokens = trimmed.split_whitespace();
            let endpoint = tokens.next()?;
            let location = tokens.next()?;
            if !location.starts_with(&format!("{country_code}-")) {
                return None;
            }

            normalize_proxy_candidate_with_scheme(endpoint, default_scheme)
        })
        .take(MAX_PROXY_CANDIDATES_PER_SOURCE)
        .collect()
}

fn parse_openproxylist_candidates(
    body: &str,
    default_scheme: &str,
    country_code: &str,
) -> Vec<String> {
    body.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed.starts_with("Fromat:")
                || trimmed.starts_with("Website=")
            {
                return None;
            }

            let mut tokens = trimmed.split_whitespace();
            let _flag = tokens.next()?;
            let endpoint = tokens.next()?;
            if !tokens.any(|token| token == country_code) {
                return None;
            }

            normalize_proxy_candidate_with_scheme(endpoint, default_scheme)
        })
        .take(MAX_PROXY_CANDIDATES_PER_SOURCE)
        .collect()
}

fn proxy_scheme_for_protocol(protocol: &str) -> Option<&'static str> {
    match protocol.trim().to_ascii_lowercase().as_str() {
        "http" | "https" => Some("http"),
        "socks4" => Some("socks4"),
        "socks5" => Some("socks5"),
        _ => None,
    }
}

fn parse_freeproxy24_candidates(body: &str, country_code: &str) -> Result<Vec<String>> {
    let entries: Vec<FreeProxy24Entry> =
        serde_json::from_str(body).context("failed to parse FreeProxy24 proxy list JSON")?;

    Ok(entries
        .into_iter()
        .filter(|entry| entry.country.eq_ignore_ascii_case(country_code))
        .filter_map(|entry| {
            let scheme = entry
                .protocols
                .iter()
                .find_map(|protocol| proxy_scheme_for_protocol(protocol))?;
            normalize_proxy_candidate_with_scheme(
                &format!("{}:{}", entry.ip.trim(), entry.port.trim()),
                scheme,
            )
        })
        .take(MAX_PROXY_CANDIDATES_PER_SOURCE)
        .collect())
}

fn parse_proxy_source_candidates(body: &str, format: ProxySourceFormat) -> Result<Vec<String>> {
    match format {
        ProxySourceFormat::PlainText => Ok(split_configured_values(body)
            .filter_map(normalize_proxy_candidate)
            .take(MAX_PROXY_CANDIDATES_PER_SOURCE)
            .collect()),
        ProxySourceFormat::SpysPlainText {
            default_scheme,
            country_code,
        } => Ok(parse_spys_proxy_candidates(
            body,
            default_scheme,
            country_code,
        )),
        ProxySourceFormat::OpenProxyListText {
            default_scheme,
            country_code,
        } => Ok(parse_openproxylist_candidates(
            body,
            default_scheme,
            country_code,
        )),
        ProxySourceFormat::FreeProxy24Json { country_code } => {
            parse_freeproxy24_candidates(body, country_code)
        }
    }
}

async fn fetch_proxy_source_candidates(
    source_client: &Client,
    source: &ProxySource,
) -> Result<Vec<String>> {
    let response = source_client
        .get(source.url.as_ref())
        .send()
        .await
        .with_context(|| format!("failed to fetch proxy source {}", source.url))?
        .error_for_status()
        .with_context(|| {
            format!(
                "proxy source returned an unsuccessful status: {}",
                source.url
            )
        })?;

    let body = response.text().await.with_context(|| {
        format!(
            "failed to read proxy source response body from {}",
            source.url
        )
    })?;

    parse_proxy_source_candidates(&body, source.format)
        .with_context(|| format!("failed to parse proxy candidates from {}", source.url))
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

fn web_base_url(base_url: &str) -> Result<Url> {
    let mut url = Url::parse(base_url).context("HH base URL must be a valid absolute URL")?;

    if let Some(host) = url.host_str().map(str::to_owned) {
        if host == "api.hh.ru" {
            url.set_host(Some("hh.ru"))
                .expect("hh.ru must be a valid replacement host");
        } else if host.starts_with("api.") && host.ends_with(".hh.ru") {
            let replacement = host["api.".len()..].to_owned();
            url.set_host(Some(&replacement))
                .expect("HH host replacement must remain valid");
        }
    }

    url.set_path("");
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

fn feed_request_url(base_url: &str) -> Result<Url> {
    let base = web_base_url(base_url)?;
    let mut request_url = base
        .join("/search/vacancy/rss")
        .context("failed to build HeadHunter RSS URL")?;
    request_url
        .query_pairs_mut()
        .append_pair("order_by", "publication_time")
        .append_pair("ored_clusters", "true")
        .append_pair("search_field", "name")
        .append_pair("text", SEARCH_QUERY);
    Ok(request_url)
}

fn vacancy_id_from_link(link: &str) -> Option<String> {
    let url = Url::parse(link).ok()?;
    let segments: Vec<_> = url.path_segments()?.collect();
    segments
        .windows(2)
        .find(|window| window[0] == "vacancy")
        .map(|window| window[1].to_owned())
}

fn parse_rss_jobs(body: &str, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<Vec<Job>> {
    let feed: SearchRssFeed = from_str(body).context("failed to parse HeadHunter search RSS")?;

    let mut jobs = Vec::new();
    for item in feed.channel.item {
        let published_at = DateTime::parse_from_rfc3339(&item.pub_date)
            .with_context(|| {
                format!(
                    "failed to parse RSS publication timestamp {}",
                    item.pub_date
                )
            })?
            .with_timezone(&Utc);
        if published_at < from || published_at > to {
            continue;
        }

        let id = match vacancy_id_from_link(&item.link) {
            Some(id) => id,
            None => {
                log::warn!(
                    "Skipping RSS item with unexpected vacancy link {}",
                    item.link
                );
                continue;
            }
        };

        jobs.push(Job {
            id,
            name: item.title,
            url: item.link,
            snippet: None,
        });
    }

    Ok(jobs)
}

async fn probe_search_access(client: &Client, base_url: &str, user_agent: &str) -> bool {
    let request_url = match feed_request_url(base_url) {
        Ok(url) => url,
        Err(error) => {
            log::warn!("Failed to build HeadHunter RSS probe URL: {error:#}");
            return false;
        }
    };

    match client
        .get(request_url)
        .header(USER_AGENT, user_agent)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => true,
        Ok(response) => {
            log::debug!("HH RSS probe received status {}", response.status());
            false
        }
        Err(error) => {
            log::debug!("HH RSS probe failed: {error}");
            false
        }
    }
}

fn uses_hh_host(base_url: &str) -> bool {
    Url::parse(base_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .is_some_and(|host| host == "hh.ru" || host.ends_with(".hh.ru"))
}

async fn configured_proxy_candidates(timeout: Duration, base_url: &str) -> Result<Vec<String>> {
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();

    if let Ok(raw) = std::env::var(PROXY_URLS_ENV_VAR) {
        for candidate in split_configured_values(&raw).filter_map(normalize_proxy_candidate) {
            if seen.insert(candidate.clone()) {
                candidates.push(candidate);
            }
        }
    }

    let source_client = build_client_with_timeout(timeout)?;
    let configured_sources = std::env::var(PROXY_SOURCE_URLS_ENV_VAR)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(|raw_sources| {
            split_configured_values(&raw_sources)
                .map(|source_url| ProxySource {
                    url: Cow::Owned(source_url.to_owned()),
                    format: ProxySourceFormat::PlainText,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            if uses_hh_host(base_url) {
                DEFAULT_PROXY_SOURCES.to_vec()
            } else {
                Vec::new()
            }
        });

    for source in configured_sources {
        match fetch_proxy_source_candidates(&source_client, &source).await {
            Ok(source_candidates) => {
                if source_candidates.is_empty() {
                    log::warn!("Proxy source {} produced no usable candidates", source.url);
                    continue;
                }

                log::info!(
                    "Loaded {} proxy candidate(s) from {}",
                    source_candidates.len(),
                    source.url
                );

                for candidate in source_candidates {
                    if seen.insert(candidate.clone()) {
                        candidates.push(candidate);
                    }

                    if candidates.len() >= MAX_PROXY_CANDIDATES {
                        log::info!(
                            "Reached proxy candidate cap of {} entries",
                            MAX_PROXY_CANDIDATES
                        );
                        return Ok(candidates);
                    }
                }
            }
            Err(error) => {
                log::warn!("Skipping proxy source {}: {error:#}", source.url);
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
        let proxy_candidates = configured_proxy_candidates(timeout, &base_url).await?;
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

            if probe_search_access(&client, &base_url, &user_agent).await {
                log::info!("Accepted HeadHunter proxy candidate {redacted}");
                routes.push(HhRoute {
                    client,
                    label: redacted,
                });
                if routes.len() >= MAX_WORKING_PROXY_ROUTES {
                    log::info!(
                        "Collected {} working HeadHunter proxy routes, stopping further probes",
                        MAX_WORKING_PROXY_ROUTES
                    );
                    break;
                }
                continue;
            }

            log::warn!("Proxy {redacted} did not pass the HeadHunter RSS probe");
        }

        if routes.is_empty() {
            log::warn!(
                "No configured proxy passed the HeadHunter RSS probe, falling back to direct access"
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

    pub async fn fetch_jobs(&self) -> Result<Vec<Job>> {
        let to = Utc::now();
        let from = to - chrono::Duration::minutes(45);
        self.fetch_jobs_between(from, to).await
    }

    pub async fn fetch_jobs_between(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<Job>> {
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
                    log::warn!("HeadHunter fetch failed via {}: {error:#}", route.label);
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
    ) -> Result<Vec<Job>> {
        let request_url = feed_request_url(&self.base_url)?;
        log::debug!("Requesting jobs from {request_url}");

        let body = client
            .get(request_url)
            .header(USER_AGENT, &self.user_agent)
            .send()
            .await
            .context("failed to request HeadHunter search RSS")?
            .error_for_status()
            .context("HeadHunter search RSS returned an unsuccessful status")?
            .text()
            .await
            .context("failed to read HeadHunter search RSS response body")?;

        let jobs = parse_rss_jobs(&body, from, to)?;
        log::debug!("Parsed {} jobs from HeadHunter RSS", jobs.len());
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
    use super::{
        normalize_proxy_candidate, parse_freeproxy24_candidates, parse_openproxylist_candidates,
        parse_rss_jobs, parse_spys_proxy_candidates, redact_proxy_url, split_configured_values,
        vacancy_id_from_link,
    };
    use chrono::{TimeZone, Utc};

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

    #[test]
    fn parse_spys_proxy_candidates_filters_russian_rows() {
        let body = "\
109.168.173.173:1080 RU-H -\n\
151.243.109.249:1082 NL-H! -\n\
31.29.180.13:1080 RU-H -\n";

        let parsed = parse_spys_proxy_candidates(body, "socks5", "RU");

        assert_eq!(
            parsed,
            vec![
                "socks5://109.168.173.173:1080",
                "socks5://31.29.180.13:1080"
            ]
        );
    }

    #[test]
    fn parse_openproxylist_candidates_filters_russian_rows() {
        let body = "\
Fromat: CountryFlag IP:PORT ResponseTime CountryCode [ISP]\n\
🇷🇺 87.117.11.57:1080 240ms RU [Macroregional South]\n\
🇧🇷 187.63.9.62:63253 468ms BR [Provider]\n\
🇷🇺 94.198.55.86:2906 250ms RU [Provider]\n";

        let parsed = parse_openproxylist_candidates(body, "socks5", "RU");

        assert_eq!(
            parsed,
            vec!["socks5://87.117.11.57:1080", "socks5://94.198.55.86:2906"]
        );
    }

    #[test]
    fn parse_freeproxy24_candidates_uses_protocol_scheme() {
        let body = r#"[
            {"ip":"85.143.173.198","port":"8444","country":"RU","protocols":["socks5"]},
            {"ip":"31.135.91.9","port":"4145","country":"RU","protocols":["socks4"]},
            {"ip":"8.8.8.8","port":"8080","country":"US","protocols":["http"]}
        ]"#;

        let parsed = parse_freeproxy24_candidates(body, "RU").unwrap();

        assert_eq!(
            parsed,
            vec!["socks5://85.143.173.198:8444", "socks4://31.135.91.9:4145"]
        );
    }

    #[test]
    fn vacancy_id_is_parsed_from_link() {
        assert_eq!(
            vacancy_id_from_link("https://spb.hh.ru/vacancy/132168741"),
            Some("132168741".to_owned())
        );
    }

    #[test]
    fn parse_rss_jobs_filters_by_requested_window() {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0">
  <channel>
    <item>
      <pubDate>2026-04-20T12:29:41.773+03:00</pubDate>
      <title>Middle Backend разработчик (Rust)</title>
      <link>https://spb.hh.ru/vacancy/132235061</link>
    </item>
    <item>
      <pubDate>2026-04-15T14:43:52.856+03:00</pubDate>
      <title>Backend-разработчик (RUST, C/C++)</title>
      <link>https://spb.hh.ru/vacancy/132168741</link>
    </item>
  </channel>
</rss>"#;
        let from = Utc.with_ymd_and_hms(2026, 4, 16, 0, 0, 0).unwrap();
        let to = Utc.with_ymd_and_hms(2026, 4, 20, 23, 59, 59).unwrap();

        let jobs = parse_rss_jobs(body, from, to).unwrap();

        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, "132235061");
        assert_eq!(jobs[0].name, "Middle Backend разработчик (Rust)");
    }
}
