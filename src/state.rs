use anyhow::Result;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
}
