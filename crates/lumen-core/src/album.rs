use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const ALBUM_BATCH_SIZE: usize = 500;
pub const ALBUM_LIMIT: usize = 20_000;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum AlbumError {
    #[error("Album not found: key '{0}' does not exist or was deleted (HTTP 404)")]
    NotFound(String),
    #[error("Album operation cancelled")]
    Cancelled,
    #[error("API error during album operation: {0}")]
    Api(String),
    #[error("Invalid album link or key: {0}")]
    InvalidLink(String),
    #[error("IO/Storage error: {0}")]
    Io(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AlbumRegistryEntry {
    #[serde(rename = "album_key")]
    pub album_key: String,
    #[serde(rename = "created_at")]
    pub created_at: u64,
}

pub fn normalize_album_account(account: &str) -> String {
    let trimmed = account.trim().to_lowercase();
    if trimmed.is_empty() {
        "default".to_string()
    } else {
        trimmed
    }
}

pub fn normalize_album_name(name: &str) -> String {
    name.trim().to_lowercase()
}

pub fn is_album_key(input: &str) -> bool {
    let input = input.trim();
    input.len() > 6 && input.starts_with("AF1Qip")
}

pub fn is_album_link(input: &str) -> bool {
    let input = input.trim();
    input.starts_with("http://") || input.starts_with("https://")
}

fn extract_clean_key(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch == '?' || ch == '#' || ch == '/' || ch == '&' || ch.is_whitespace() {
            break;
        }
        out.push(ch);
    }
    out
}

pub fn parse_album_link(input: &str) -> Result<String, AlbumError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AlbumError::InvalidLink("album link cannot be empty".to_string()));
    }

    if is_album_key(trimmed) {
        let key = extract_clean_key(trimmed);
        if is_album_key(&key) {
            return Ok(key);
        }
    }

    if is_album_link(trimmed) {
        if let Some(idx) = trimmed.find("AF1Qip") {
            let key = extract_clean_key(&trimmed[idx..]);
            if is_album_key(&key) {
                return Ok(key);
            }
            return Err(AlbumError::InvalidLink("malformed album key in URL".to_string()));
        }
        return Err(AlbumError::InvalidLink(
            "invalid Google Photos album link: no AF1Qip album key found".to_string(),
        ));
    }

    Err(AlbumError::InvalidLink("input is not a valid album link or key".to_string()))
}

pub fn is_album_not_found_error(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    lower.contains("not found")
        || lower.contains("not_found")
        || lower.contains("404")
        || lower.contains("deleted")
        || lower.contains("does not exist")
        || lower.contains("invalid album")
}

#[derive(Clone)]
pub struct AlbumRegistry {
    file_path: PathBuf,
    // entries: account -> (normalized_name -> entry)
    entries: Arc<RwLock<HashMap<String, HashMap<String, AlbumRegistryEntry>>>>,
    locks: Arc<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
}

impl AlbumRegistry {
    pub fn new(file_path: PathBuf) -> Result<Self, AlbumError> {
        let reg = Self {
            file_path,
            entries: Arc::new(RwLock::new(HashMap::new())),
            locks: Arc::new(Mutex::new(HashMap::new())),
        };
        reg.load()?;
        Ok(reg)
    }

    pub fn in_memory() -> Self {
        Self {
            file_path: PathBuf::new(),
            entries: Arc::new(RwLock::new(HashMap::new())),
            locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn get_name_lock(&self, account: &str, name: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.locks.lock().unwrap();
        let key = format!("{}::{}", normalize_album_account(account), normalize_album_name(name));
        locks.entry(key).or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))).clone()
    }

    fn load(&self) -> Result<(), AlbumError> {
        if self.file_path.as_os_str().is_empty() || !self.file_path.exists() {
            return Ok(());
        }
        let data = fs::read_to_string(&self.file_path)
            .map_err(|e| AlbumError::Io(e.to_string()))?;
        if data.trim().is_empty() {
            return Ok(());
        }
        let loaded: HashMap<String, HashMap<String, AlbumRegistryEntry>> = serde_json::from_str(&data)
            .map_err(|e| AlbumError::Io(e.to_string()))?;
        let mut entries = self.entries.write().unwrap();
        *entries = loaded;
        Ok(())
    }

    fn save_locked(&self) -> Result<(), AlbumError> {
        if self.file_path.as_os_str().is_empty() {
            return Ok(());
        }
        if let Some(parent) = self.file_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let entries = self.entries.read().unwrap();
        let json = serde_json::to_string_pretty(&*entries)
            .map_err(|e| AlbumError::Io(e.to_string()))?;
        let tmp = self.file_path.with_extension("tmp");
        fs::write(&tmp, json).map_err(|e| AlbumError::Io(e.to_string()))?;
        fs::rename(&tmp, &self.file_path).map_err(|e| AlbumError::Io(e.to_string()))?;
        Ok(())
    }

    pub fn get(&self, account: &str, name: &str) -> Option<AlbumRegistryEntry> {
        let entries = self.entries.read().unwrap();
        let acct_key = normalize_album_account(account);
        let name_key = normalize_album_name(name);
        entries.get(&acct_key)?.get(&name_key).cloned()
    }

    pub fn set(&self, account: &str, name: &str, album_key: &str) -> Result<(), AlbumError> {
        let acct_key = normalize_album_account(account);
        let name_key = normalize_album_name(name);
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

        {
            let mut entries = self.entries.write().unwrap();
            let acct_map = entries.entry(acct_key).or_default();
            acct_map.insert(
                name_key,
                AlbumRegistryEntry {
                    album_key: album_key.to_string(),
                    created_at: now,
                },
            );
        }
        self.save_locked()
    }

    pub fn delete(&self, account: &str, name: &str) -> Result<(), AlbumError> {
        let acct_key = normalize_album_account(account);
        let name_key = normalize_album_name(name);

        {
            let mut entries = self.entries.write().unwrap();
            if let Some(acct_map) = entries.get_mut(&acct_key) {
                acct_map.remove(&name_key);
            }
        }
        self.save_locked()
    }

    pub fn find_name_by_key(&self, account: &str, key: &str) -> Option<String> {
        let entries = self.entries.read().unwrap();
        let acct_key = normalize_album_account(account);
        let acct_map = entries.get(&acct_key)?;
        for (name, entry) in acct_map {
            if entry.album_key == key {
                return Some(name.clone());
            }
        }
        None
    }

    pub fn get_all_nicknames(&self, account: &str) -> HashMap<String, String> {
        let entries = self.entries.read().unwrap();
        let acct_key = normalize_album_account(account);
        let mut map = HashMap::new();
        if let Some(acct_map) = entries.get(&acct_key) {
            for (name, entry) in acct_map {
                map.insert(entry.album_key.clone(), name.clone());
            }
        }
        map
    }
}

#[async_trait::async_trait]
pub trait AlbumApi: Send + Sync {
    async fn create_album(&self, album_name: &str, media_keys: &[String]) -> Result<String, AlbumError>;
    async fn add_media_to_album(&self, album_media_key: &str, media_keys: &[String]) -> Result<(), AlbumError>;
}

pub struct AlbumManager {
    api: Arc<dyn AlbumApi>,
    account_email: String,
    registry: AlbumRegistry,
}

impl AlbumManager {
    pub fn new(api: Arc<dyn AlbumApi>, account_email: impl Into<String>, registry: AlbumRegistry) -> Self {
        Self {
            api,
            account_email: account_email.into(),
            registry,
        }
    }

    pub async fn add_to_album(&self, media_keys: &[String], album_name_or_key: &str) -> Result<Vec<String>, AlbumError> {
        if media_keys.is_empty() {
            return Ok(Vec::new());
        }
        let trimmed = album_name_or_key.trim();
        if trimmed.is_empty() {
            return Err(AlbumError::InvalidLink("album name or key cannot be empty".to_string()));
        }

        // Direct key or link
        if is_album_link(trimmed) || is_album_key(trimmed) {
            let key = parse_album_link(trimmed)?;
            self.add_media_in_batches(&key, media_keys).await?;
            return Ok(vec![key]);
        }

        // Multiple albums if exceeding ALBUM_LIMIT
        let mut all_album_keys = Vec::new();

        for (idx, chunk) in media_keys.chunks(ALBUM_LIMIT).enumerate() {
            let counter = idx + 1;
            let current_name = if media_keys.len() > ALBUM_LIMIT {
                format!("{} ({})", trimmed, counter)
            } else {
                trimmed.to_string()
            };

            let key = self.get_or_create_and_add(chunk, &current_name).await?;
            all_album_keys.push(key);
        }

        Ok(all_album_keys)
    }

    async fn get_or_create_and_add(&self, media_keys: &[String], album_name: &str) -> Result<String, AlbumError> {
        let lock = self.registry.get_name_lock(&self.account_email, album_name);
        let _guard = lock.lock().await;

        if let Some(entry) = self.registry.get(&self.account_email, album_name) {
            match self.add_media_in_batches(&entry.album_key, media_keys).await {
                Ok(()) => return Ok(entry.album_key),
                Err(AlbumError::NotFound(_k)) => {
                    // Deleted or missing: clear registry and recreate once
                    let _ = self.registry.delete(&self.account_email, album_name);
                    return self.create_and_add_in_batches(media_keys, album_name).await;
                }
                Err(e) => return Err(e),
            }
        }

        self.create_and_add_in_batches(media_keys, album_name).await
    }

    async fn create_and_add_in_batches(&self, media_keys: &[String], album_name: &str) -> Result<String, AlbumError> {
        let first_end = std::cmp::min(ALBUM_BATCH_SIZE, media_keys.len());
        let first_batch = &media_keys[..first_end];

        let album_key = self.api.create_album(album_name, first_batch).await?;
        self.registry.set(&self.account_email, album_name, &album_key)?;

        if media_keys.len() > first_end {
            self.add_media_in_batches(&album_key, &media_keys[first_end..]).await?;
        }

        Ok(album_key)
    }

    async fn add_media_in_batches(&self, album_key: &str, media_keys: &[String]) -> Result<(), AlbumError> {
        for chunk in media_keys.chunks(ALBUM_BATCH_SIZE) {
            self.api.add_media_to_album(album_key, chunk).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockAlbumApi {
        created: Mutex<Vec<(String, Vec<String>)>>,
        added: Mutex<Vec<(String, Vec<String>)>>,
        deleted_keys: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl AlbumApi for MockAlbumApi {
        async fn create_album(&self, album_name: &str, media_keys: &[String]) -> Result<String, AlbumError> {
            let key = format!("AF1Qip_{}", album_name);
            self.created.lock().unwrap().push((album_name.to_string(), media_keys.to_vec()));
            Ok(key)
        }

        async fn add_media_to_album(&self, album_media_key: &str, media_keys: &[String]) -> Result<(), AlbumError> {
            if self.deleted_keys.lock().unwrap().contains(&album_media_key.to_string()) {
                return Err(AlbumError::NotFound(album_media_key.to_string()));
            }
            self.added.lock().unwrap().push((album_media_key.to_string(), media_keys.to_vec()));
            Ok(())
        }
    }

    #[test]
    fn test_parse_album_link() {
        assert_eq!(
            parse_album_link("AF1QipM47samplekey123").unwrap(),
            "AF1QipM47samplekey123"
        );
        assert_eq!(
            parse_album_link("https://photos.google.com/album/AF1QipP_sample123?authuser=0").unwrap(),
            "AF1QipP_sample123"
        );
        assert!(parse_album_link("https://photos.google.com/u/0/").is_err());
    }

    #[tokio::test]
    async fn test_album_manager_create_and_dedup() {
        let api = Arc::new(MockAlbumApi {
            created: Mutex::new(Vec::new()),
            added: Mutex::new(Vec::new()),
            deleted_keys: Mutex::new(Vec::new()),
        });
        let reg = AlbumRegistry::in_memory();
        let mgr = AlbumManager::new(api.clone(), "test@example.com", reg);

        let keys = vec!["key1".to_string(), "key2".to_string()];
        let album_keys = mgr.add_to_album(&keys, "Vacation 2026").await.unwrap();

        assert_eq!(album_keys.len(), 1);
        let first_key = &album_keys[0];
        assert_eq!(first_key, "AF1Qip_Vacation 2026");

        // Second call with same name should REUSE existing album without creating duplicate
        let more_keys = vec!["key3".to_string()];
        let album_keys_2 = mgr.add_to_album(&more_keys, "Vacation 2026").await.unwrap();
        assert_eq!(album_keys_2[0], *first_key);

        assert_eq!(api.created.lock().unwrap().len(), 1);
        assert_eq!(api.added.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_album_manager_recreates_if_not_found() {
        let api = Arc::new(MockAlbumApi {
            created: Mutex::new(Vec::new()),
            added: Mutex::new(Vec::new()),
            deleted_keys: Mutex::new(vec!["AF1Qip_Vacation".to_string()]),
        });
        let reg = AlbumRegistry::in_memory();
        reg.set("test@example.com", "Vacation", "AF1Qip_Vacation").unwrap();

        let mgr = AlbumManager::new(api.clone(), "test@example.com", reg);
        let keys = vec!["key1".to_string()];

        // When existing key is 404/deleted, it deletes from registry and creates once
        let album_keys = mgr.add_to_album(&keys, "Vacation").await.unwrap();
        assert_eq!(album_keys[0], "AF1Qip_Vacation");
        assert_eq!(api.created.lock().unwrap().len(), 1);
    }
}
