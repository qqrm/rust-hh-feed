use anyhow::Result;
use chrono::{Duration, NaiveDate, Utc};
use std::{collections::HashMap, fs, path::Path};

pub type PostedJobs = HashMap<String, String>;

pub fn load_posted_jobs(path: &Path) -> Result<PostedJobs> {
    if path.exists() {
        let data = fs::read_to_string(path)?;
        let map = serde_json::from_str(&data)?;
        Ok(map)
    } else {
        Ok(HashMap::new())
    }
}

pub fn save_posted_jobs(path: &Path, map: &PostedJobs) -> Result<()> {
    let data = serde_json::to_string_pretty(map)?;
    fs::write(path, data)?;
    Ok(())
}

/// Remove entries older than `retention_days`.
pub fn prune_old_jobs(map: &mut PostedJobs, retention_days: i64) {
    let cutoff = Utc::now().date_naive() - Duration::days(retention_days);
    map.retain(|_, date| {
        NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map(|d| d >= cutoff)
            .unwrap_or(false)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn load_and_save_cycle() {
        let path = PathBuf::from("test_posted_jobs.json");
        let mut map = PostedJobs::new();
        map.insert("1".into(), "2024-07-08".into());
        save_posted_jobs(&path, &map).unwrap();
        let loaded = load_posted_jobs(&path).unwrap();
        assert_eq!(map, loaded);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn load_missing_returns_empty() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("missing.json");
        let map = load_posted_jobs(&path).unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn load_invalid_json_errors() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bad.json");
        fs::write(&path, "not json").unwrap();
        let result = load_posted_jobs(&path);
        assert!(result.is_err());
    }

    #[test]
    fn save_fails_on_directory() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("subdir");
        fs::create_dir(&path).unwrap();
        let map = PostedJobs::new();
        let result = save_posted_jobs(&path, &map);
        assert!(result.is_err());
    }

    #[test]
    fn prune_old_jobs_removes_outdated_entries() {
        let mut map = PostedJobs::new();
        map.insert("old".into(), "2024-01-01".into());
        let recent = Utc::now().date_naive().to_string();
        map.insert("recent".into(), recent.clone());
        prune_old_jobs(&mut map, 30);
        assert!(map.contains_key("recent"));
        assert!(!map.contains_key("old"));
    }

    #[test]
    fn prune_old_jobs_drops_invalid_dates() {
        let mut map = PostedJobs::new();
        map.insert("bad".into(), "not a date".into());
        prune_old_jobs(&mut map, 30);
        assert!(map.is_empty());
    }
}
