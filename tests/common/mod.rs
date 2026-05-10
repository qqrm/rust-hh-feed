use chrono::{Duration, Utc};

pub fn recent_rfc3339(minutes_ago: i64) -> String {
    (Utc::now() - Duration::minutes(minutes_ago)).to_rfc3339()
}

pub fn rss_feed(items: &[(&str, &str, &str)]) -> String {
    let items_xml = items
        .iter()
        .map(|(pub_date, title, link)| {
            format!(
                "<item><pubDate>{pub_date}</pubDate><title>{title}</title><link>{link}</link><guid isPermaLink=\"true\">{link}</guid></item>"
            )
        })
        .collect::<Vec<_>>()
        .join("");

    format!(
        "<?xml version='1.0' encoding='utf-8'?><rss version=\"2.0\"><channel>{items_xml}</channel></rss>"
    )
}
