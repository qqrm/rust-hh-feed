//! Fail closed around every externally visible send.

use crate::delivery_state::{self, Operation};
use anyhow::Result;
use std::fs;
use std::io;
use std::path::Path;

fn require_clear(path: &Path) -> io::Result<()> {
    match fs::metadata(delivery_state::pending_path(path)) {
        Ok(_) => Err(io::Error::other(
            "Unconfirmed delivery pending; reconcile before retrying",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("checkpoint-tmp");
    let mut file = fs::File::create(&temporary)?;
    use std::io::Write;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary, path)
}

pub async fn save(path: &Path, bytes: &[u8]) -> Result<()> {
    require_clear(path)?;
    write(path, bytes)?;
    if let Some(config) = delivery_state::config_from_env()? {
        delivery_state::commit(&config, path, Operation::Save, None).await?;
    }
    Ok(())
}

pub async fn begin(path: &Path, identity: &str) -> Result<()> {
    require_clear(path)?;
    write(
        &delivery_state::pending_path(path),
        b"Unconfirmed delivery; reconcile before retrying\n",
    )?;
    if let Some(config) = delivery_state::config_from_env()? {
        delivery_state::commit(&config, path, Operation::Begin, Some(identity)).await?;
    }
    Ok(())
}

pub async fn confirm(path: &Path, bytes: &[u8]) -> Result<()> {
    write(path, bytes)?;
    if let Some(config) = delivery_state::config_from_env()? {
        delivery_state::commit(&config, path, Operation::Confirm, None).await?;
    }
    fs::remove_file(delivery_state::pending_path(path))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn interrupted_send_and_failed_confirmation_block_retry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state");
        save(&path, b"before").await.unwrap();
        begin(&path, "item-1").await.unwrap();
        assert!(begin(&path, "item-1").await.is_err());
        assert!(save(&path, b"before").await.is_err());
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(confirm(&path, b"acknowledged").await.is_err());
        assert!(begin(&path, "item-1").await.is_err());
        std::fs::remove_dir(&path).unwrap();
        confirm(&path, b"acknowledged").await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"acknowledged");
        save(&path, b"acknowledged").await.unwrap();
    }
}
