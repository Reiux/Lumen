use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use serde::{Deserialize, Serialize};

use crate::keyring::{KeyringStorage, MockKeyringProvider};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WatchedFolderConfig {
    #[serde(default)]
    pub path: String,
    #[serde(default, rename = "account_email")]
    pub account_email: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, rename = "include_subfolders")]
    pub include_subfolders: bool,
    #[serde(default, rename = "album_mode")]
    pub album_mode: String, // "none", "auto", "custom"
    #[serde(default, rename = "album_name_or_key")]
    pub album_name_or_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Preferences {
    #[serde(default)]
    pub proxy: String,
    #[serde(default, rename = "use_quota")]
    pub use_quota: bool,
    #[serde(default)]
    pub saver: bool,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default, rename = "force_upload")]
    pub force_upload: bool,
    #[serde(default, rename = "pair_live_photos")]
    pub pair_live_photos: bool,
    #[serde(default = "default_true", rename = "skip_incomplete_live_photos")]
    pub skip_incomplete_live_photos: bool,
    #[serde(default, rename = "update_existing_photos_to_live")]
    pub update_existing_photos_to_live: bool,
    #[serde(default = "default_threads", rename = "upload_threads")]
    pub upload_threads: usize,
    #[serde(default, rename = "delete_from_host")]
    pub delete_from_host: bool,
    #[serde(default, rename = "disable_unsupported_files_filter")]
    pub disable_unsupported_files_filter: bool,
    #[serde(default, rename = "set_date_from_filename")]
    pub set_date_from_filename: bool,
    #[serde(default, rename = "exclude_pattern")]
    pub exclude_pattern: String,
    #[serde(default, rename = "minimize_to_tray")]
    pub minimize_to_tray: bool,
    #[serde(default, rename = "launch_on_startup")]
    pub launch_on_startup: bool,
    #[serde(default = "default_true", rename = "notify_on_complete")]
    pub notify_on_complete: bool,
    #[serde(default, rename = "use_local_hash_cache")]
    pub use_local_hash_cache: bool,
    #[serde(default, rename = "upload_speed_limit_kbps")]
    pub upload_speed_limit_kbps: u32,
    #[serde(default, rename = "watch_folders_enabled")]
    pub watch_folders_enabled: bool,
    #[serde(default, rename = "watched_folders")]
    pub watched_folders: Vec<String>,
    #[serde(default, rename = "watched_folder_configs")]
    pub watched_folder_configs: Vec<WatchedFolderConfig>,
    #[serde(default, rename = "recent_albums")]
    pub recent_albums: Vec<String>,
    #[serde(default = "default_true", rename = "auto_check_updates")]
    pub auto_check_updates: bool,
    #[serde(default, rename = "last_update_check", deserialize_with = "deserialize_last_update_check")]
    pub last_update_check: i64,
    #[serde(default, skip_serializing)]
    pub album_name: String,
    #[serde(default, skip_serializing)]
    pub album_auto_mode: bool,
}

fn deserialize_last_update_check<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct LastUpdateCheckVisitor;

    impl<'de> serde::de::Visitor<'de> for LastUpdateCheckVisitor {
        type Value = i64;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("an integer timestamp or a string timestamp")
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(v)
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(v as i64)
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(v.parse::<i64>().unwrap_or(0))
        }

        fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(v.parse::<i64>().unwrap_or(0))
        }
    }

    deserializer.deserialize_any(LastUpdateCheckVisitor)
}

fn default_true() -> bool {
    true
}

fn default_threads() -> usize {
    3
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            proxy: String::new(),
            use_quota: false,
            saver: false,
            recursive: false,
            force_upload: false,
            pair_live_photos: false,
            skip_incomplete_live_photos: true,
            update_existing_photos_to_live: false,
            upload_threads: 3,
            delete_from_host: false,
            disable_unsupported_files_filter: false,
            set_date_from_filename: false,
            exclude_pattern: String::new(),
            minimize_to_tray: false,
            launch_on_startup: false,
            notify_on_complete: true,
            use_local_hash_cache: false,
            upload_speed_limit_kbps: 0,
            watch_folders_enabled: false,
            watched_folders: Vec::new(),
            watched_folder_configs: Vec::new(),
            recent_albums: Vec::new(),
            auto_check_updates: true,
            last_update_check: 0,
            album_name: String::new(),
            album_auto_mode: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AccountConfig {
    #[serde(default)]
    pub credentials: Vec<String>,
    #[serde(default)]
    pub selected: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Config {
    #[serde(default)]
    pub account: AccountConfig,
    #[serde(default)]
    pub preferences: Preferences,
}

/// Legacy flat config format for backward compatibility with older gotohp.config
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LegacyFlatConfig {
    #[serde(default)]
    pub credentials: Vec<String>,
    #[serde(default)]
    pub selected: String,
    #[serde(default)]
    pub proxy: String,
    #[serde(default, rename = "use_quota")]
    pub use_quota: bool,
    #[serde(default)]
    pub saver: bool,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default, rename = "force_upload")]
    pub force_upload: bool,
    #[serde(default, rename = "pair_live_photos")]
    pub pair_live_photos: bool,
    #[serde(default = "default_true", rename = "skip_incomplete_live_photos")]
    pub skip_incomplete_live_photos: bool,
    #[serde(default, rename = "update_existing_photos_to_live")]
    pub update_existing_photos_to_live: bool,
    #[serde(default = "default_threads", rename = "upload_threads")]
    pub upload_threads: usize,
    #[serde(default, rename = "delete_from_host")]
    pub delete_from_host: bool,
    #[serde(default, rename = "disable_unsupported_files_filter")]
    pub disable_unsupported_files_filter: bool,
    #[serde(default, rename = "set_date_from_filename")]
    pub set_date_from_filename: bool,
    #[serde(default, rename = "exclude_pattern")]
    pub exclude_pattern: String,
}

impl LegacyFlatConfig {
    #[allow(clippy::wrong_self_convention)]
    fn to_config(self) -> Config {
        Config {
            account: AccountConfig {
                credentials: self.credentials,
                selected: self.selected,
            },
            preferences: Preferences {
                proxy: self.proxy,
                use_quota: self.use_quota,
                saver: self.saver,
                recursive: self.recursive,
                force_upload: self.force_upload,
                pair_live_photos: self.pair_live_photos,
                skip_incomplete_live_photos: self.skip_incomplete_live_photos,
                update_existing_photos_to_live: self.update_existing_photos_to_live,
                upload_threads: self.upload_threads,
                delete_from_host: self.delete_from_host,
                disable_unsupported_files_filter: self.disable_unsupported_files_filter,
                set_date_from_filename: self.set_date_from_filename,
                exclude_pattern: self.exclude_pattern,
                ..Default::default()
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveCredentialResult {
    pub keyring_saved: bool,
    pub fallback_saved: bool,
}

pub struct ConfigManager {
    config: RwLock<Config>,
    config_path: RwLock<PathBuf>,
    keyring: Arc<KeyringStorage>,
}

static GLOBAL_CONFIG_MANAGER: OnceLock<ConfigManager> = OnceLock::new();

impl ConfigManager {
    pub fn new(config_path: PathBuf, keyring: Arc<KeyringStorage>) -> Self {
        let config = Self::load_from_path(&config_path).unwrap_or_default();
        Self {
            config: RwLock::new(config),
            config_path: RwLock::new(config_path),
            keyring,
        }
    }

    pub fn new_with_mock_keyring(config_path: PathBuf) -> Self {
        Self::new(config_path, Arc::new(KeyringStorage::new(Arc::new(MockKeyringProvider::new()))))
    }

    pub fn global() -> &'static ConfigManager {
        GLOBAL_CONFIG_MANAGER.get_or_init(|| {
            let path = determine_config_path(None);
            ConfigManager::new(path, Arc::new(KeyringStorage::new_os()))
        })
    }

    pub fn get_config(&self) -> Config {
        self.config.read().unwrap().clone()
    }

    pub fn get_preferences(&self) -> Preferences {
        self.config.read().unwrap().preferences.clone()
    }

    pub fn get_config_path(&self) -> PathBuf {
        self.config_path.read().unwrap().clone()
    }

    pub fn update<F>(&self, f: F) -> Result<(), String>
    where
        F: FnOnce(&mut Config),
    {
        let mut cfg = self.config.write().unwrap();
        f(&mut cfg);
        let path = self.config_path.read().unwrap().clone();
        Self::save_to_path(&cfg, &path)
    }

    pub fn load_from_path(path: &Path) -> Result<Config, String> {
        if !path.exists() {
            return Ok(Config::default());
        }
        let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err("Config file is a symlink or not a regular file".to_string());
        }
        if meta.len() > 5 * 1024 * 1024 {
            return Err("Config file exceeds maximum allowed size (5 MB)".to_string());
        }
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        if content.trim().is_empty() {
            return Ok(Config::default());
        }

        // Parse into generic YAML value to detect legacy flat format vs nested format
        if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(&content) {
            if let serde_yaml::Value::Mapping(ref map) = val {
                let account_key = serde_yaml::Value::String("account".to_string());
                let preferences_key = serde_yaml::Value::String("preferences".to_string());
                let has_account = map.contains_key(&account_key);
                let has_preferences = map.contains_key(&preferences_key);

                if has_account || has_preferences {
                    if let Ok(config) = serde_yaml::from_value::<Config>(val) {
                        return Ok(config);
                    }
                } else {
                    // Legacy flat format
                    if let Ok(flat) = serde_yaml::from_value::<LegacyFlatConfig>(val.clone()) {
                        return Ok(flat.to_config());
                    }
                }
            }
        }

        // Try standard nested Config
        if let Ok(config) = serde_yaml::from_str::<Config>(&content) {
            return Ok(config);
        }

        // Try JSON format as well
        if let Ok(config) = serde_json::from_str::<Config>(&content) {
            return Ok(config);
        }

        Ok(Config::default())
    }

    pub fn save_to_path(config: &Config, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
            }
        }

        let yaml = serde_yaml::to_string(config).map_err(|e| e.to_string())?;
        let temp_path = path.with_extension("tmp");
        let _ = fs::remove_file(&temp_path);

        {
            use std::io::Write;
            let mut opts = fs::OpenOptions::new();
            opts.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
                opts.custom_flags(libc::O_NOFOLLOW);
            }
            let mut f = opts.open(&temp_path).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = f.set_permissions(fs::Permissions::from_mode(0o600));
            }
            f.write_all(yaml.as_bytes()).map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
        }

        fs::rename(&temp_path, path).map_err(|e| e.to_string())?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
        }

        Ok(())
    }

    pub fn get_credential_for_account(&self, email_opt: Option<&str>) -> Result<String, String> {
        let email = match email_opt {
            Some(e) if !e.trim().is_empty() => e.trim().to_string(),
            _ => {
                let cfg = self.config.read().unwrap();
                cfg.account.selected.clone()
            }
        };

        if email.is_empty() {
            return Err("no account email specified or selected".to_string());
        }

        // 1. Try OS keyring first
        if let Ok(Some(cred)) = self.keyring.get_credential(&email) {
            if !cred.trim().is_empty() {
                return Ok(cred);
            }
        }

        // 2. Fall back to config credentials list
        let cfg = self.config.read().unwrap();
        for cred in &cfg.account.credentials {
            if let Some(parsed) = url::form_urlencoded::parse(cred.as_bytes()).into_owned().collect::<std::collections::HashMap<_, _>>().get("Email").cloned() {
                if parsed.eq_ignore_ascii_case(&email) {
                    return Ok(cred.clone());
                }
            }
        }

        Err(format!("no credentials found for account: {}", email))
    }

    pub fn save_credential(&self, email: &str, credential: &str) -> Result<SaveCredentialResult, String> {
        let email = email.trim();
        if email.is_empty() {
            return Err("email cannot be empty".to_string());
        }

        // Try saving to keyring
        let keyring_result = self.keyring.store_credential(email, credential);
        let keyring_saved = keyring_result.is_ok();
        let should_backup_in_file = !keyring_saved;

        self.update(|cfg| {
            cfg.account.selected = email.to_string();
            // Remove existing entry for same email if present
            cfg.account.credentials.retain(|c| {
                if let Some((_, e)) = url::form_urlencoded::parse(c.as_bytes()).find(|(k, _)| k == "Email") {
                    !e.eq_ignore_ascii_case(email)
                } else {
                    true
                }
            });
            if should_backup_in_file {
                cfg.account.credentials.push(credential.to_string());
            } else {
                let sanitized: String = url::form_urlencoded::parse(credential.as_bytes())
                    .filter(|(k, _)| k != "Token")
                    .fold(url::form_urlencoded::Serializer::new(String::new()), |mut ser, (k, v)| {
                        ser.append_pair(&k, &v);
                        ser
                    })
                    .finish();
                cfg.account.credentials.push(sanitized);
            }
        })?;

        Ok(SaveCredentialResult {
            keyring_saved,
            fallback_saved: should_backup_in_file,
        })
    }

    pub fn remove_credential(&self, email: &str) -> Result<(), String> {
        let email = email.trim();
        let _ = self.keyring.delete_credential(email);

        self.update(|cfg| {
            cfg.account.credentials.retain(|c| {
                if let Some((_, e)) = url::form_urlencoded::parse(c.as_bytes()).find(|(k, _)| k == "Email") {
                    !e.eq_ignore_ascii_case(email)
                } else {
                    true
                }
            });
            if cfg.account.selected.eq_ignore_ascii_case(email) {
                cfg.account.selected = cfg.account.credentials.first().and_then(|c| {
                    url::form_urlencoded::parse(c.as_bytes())
                        .find(|(k, _)| k == "Email")
                        .map(|(_, v)| v.into_owned())
                }).unwrap_or_default();
            }
        })
    }
}

pub fn get_user_config_dir() -> PathBuf {
    if let Some(proj_dirs) = directories::BaseDirs::new() {
        proj_dirs.config_dir().to_path_buf()
    } else {
        PathBuf::from(".")
    }
}

pub fn determine_config_path(override_path: Option<&Path>) -> PathBuf {
    if let Some(p) = override_path {
        return p.to_path_buf();
    }

    // 1. Check next to executable (portable mode)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if let Some(p) = existing_local_config(dir) {
                return p;
            }
        }
    }

    // 2. User config directory: prefer ~/.config/lumen/lumen.config
    let user_config = get_user_config_dir();
    let lumen_dir = user_config.join("lumen");
    let lumen_config = lumen_dir.join("lumen.config");

    if lumen_config.exists() {
        return lumen_config;
    }

    // 4. Migration check: legacy gotohp directory
    let legacy_dir = user_config.join("gotohp");
    let legacy_config = legacy_dir.join("gotohp.config");

    if legacy_config.exists() {
        let legacy_meta = fs::symlink_metadata(&legacy_config).ok();
        if let Some(m) = legacy_meta {
            if m.is_file() && !m.file_type().is_symlink() {
                let _ = fs::create_dir_all(&lumen_dir);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::set_permissions(&lumen_dir, fs::Permissions::from_mode(0o700));
                }
                if fs::copy(&legacy_config, &lumen_config).is_ok() {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let _ = fs::set_permissions(&lumen_config, fs::Permissions::from_mode(0o600));
                    }
                    let legacy_db = legacy_dir.join("hash_cache.db");
                    if let Ok(db_meta) = fs::symlink_metadata(&legacy_db) {
                        if db_meta.is_file() && !db_meta.file_type().is_symlink() {
                            let target_db = lumen_dir.join("hash_cache.db");
                            let _ = fs::copy(&legacy_db, &target_db);
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::PermissionsExt;
                                let _ = fs::set_permissions(&target_db, fs::Permissions::from_mode(0o600));
                            }
                        }
                    }
                    let legacy_reg = legacy_dir.join("album_registry.json");
                    if let Ok(reg_meta) = fs::symlink_metadata(&legacy_reg) {
                        if reg_meta.is_file() && !reg_meta.file_type().is_symlink() {
                            let target_reg = lumen_dir.join("album_registry.json");
                            let _ = fs::copy(&legacy_reg, &target_reg);
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::PermissionsExt;
                                let _ = fs::set_permissions(&target_reg, fs::Permissions::from_mode(0o600));
                            }
                        }
                    }
                    return lumen_config;
                }
            }
        }
        return legacy_config;
    }

    lumen_config
}

fn existing_local_config(dir: &Path) -> Option<PathBuf> {
    let lumen = dir.join("lumen.config");
    if lumen.exists() {
        return Some(lumen);
    }
    let gotohp = dir.join("gotohp.config");
    if gotohp.exists() {
        return Some(gotohp);
    }
    None
}

pub fn determine_hash_cache_path(base_dir: &Path) -> PathBuf {
    let standard = base_dir.join("hash_cache.db");
    if standard.exists() {
        return standard;
    }
    let alternate = base_dir.join("hashes.db");
    if alternate.exists() {
        return alternate;
    }
    standard
}

pub fn determine_album_registry_path(base_dir: &Path) -> PathBuf {
    let standard = base_dir.join("album_registry.json");
    if standard.exists() {
        return standard;
    }
    let alternate = base_dir.join("albums.json");
    if alternate.exists() {
        return alternate;
    }
    standard
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_config_save_and_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("lumen.config");

        let mut config = Config::default();
        config.account.selected = "test@example.com".to_string();
        config.preferences.upload_threads = 5;
        config.preferences.saver = true;

        ConfigManager::save_to_path(&config, &path).unwrap();
        let loaded = ConfigManager::load_from_path(&path).unwrap();

        assert_eq!(loaded.account.selected, "test@example.com");
        assert_eq!(loaded.preferences.upload_threads, 5);
        assert!(loaded.preferences.saver);
    }

    #[test]
    fn test_legacy_flat_config_migration() {
        let yaml = r#"
selected: "legacy@example.com"
upload_threads: 4
saver: true
force_upload: true
"#;
        let dir = tempdir().unwrap();
        let path = dir.path().join("gotohp.config");
        fs::write(&path, yaml).unwrap();

        let loaded = ConfigManager::load_from_path(&path).unwrap();
        assert_eq!(loaded.account.selected, "legacy@example.com");
        assert_eq!(loaded.preferences.upload_threads, 4);
        assert!(loaded.preferences.saver);
        assert!(loaded.preferences.force_upload);
    }

    #[test]
    fn test_config_manager_credentials() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("lumen.config");
        let mgr = ConfigManager::new_with_mock_keyring(path);

        let email = "alpha@example.com";
        let cred = "androidId=1234&Email=alpha@example.com&Token=secret_token";

        mgr.save_credential(email, cred).unwrap();
        assert_eq!(mgr.get_config().account.selected, email);

        let retrieved = mgr.get_credential_for_account(Some(email)).unwrap();
        assert_eq!(retrieved, cred);

        let retrieved_default = mgr.get_credential_for_account(None).unwrap();
        assert_eq!(retrieved_default, cred);

        mgr.remove_credential(email).unwrap();
        assert!(mgr.get_credential_for_account(Some(email)).is_err());
    }

    #[test]
    fn test_keyring_fallback_0600_permissions_and_storage() {
        use crate::keyring::{FailingKeyringProvider, KeyringStorage};

        let dir = tempdir().unwrap();
        let path = dir.path().join("lumen.config");
        let failing_keyring = Arc::new(KeyringStorage::new(Arc::new(FailingKeyringProvider)));
        let mgr = ConfigManager::new(path.clone(), failing_keyring);

        let email = "fallback@example.com";
        let cred = "androidId=5678&Email=fallback@example.com&Token=fallback_secret";

        let result = mgr.save_credential(email, cred).unwrap();
        assert!(!result.keyring_saved, "Keyring should not have been used");
        assert!(result.fallback_saved, "Fallback storage should have been used");

        // Verify content written to file
        let file_content = fs::read_to_string(&path).unwrap();
        assert!(file_content.contains("fallback_secret"), "Config file must contain fallback credential");
        assert!(file_content.contains("fallback@example.com"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = fs::metadata(&path).unwrap();
            let mode = meta.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "Config file must be mode 0600 (-rw-------)");
        }

        // Test retrieval from fallback file
        let retrieved = mgr.get_credential_for_account(Some(email)).unwrap();
        assert_eq!(retrieved, cred);
    }

    #[test]
    fn test_determine_cache_and_album_paths() {
        let dir = tempdir().unwrap();
        let path = dir.path();

        // Default when neither exists
        assert_eq!(determine_hash_cache_path(path), path.join("hash_cache.db"));
        assert_eq!(determine_album_registry_path(path), path.join("album_registry.json"));

        // If hashes.db exists, it should fall back to it
        fs::write(path.join("hashes.db"), b"").unwrap();
        fs::write(path.join("albums.json"), b"").unwrap();
        assert_eq!(determine_hash_cache_path(path), path.join("hashes.db"));
        assert_eq!(determine_album_registry_path(path), path.join("albums.json"));

        // If hash_cache.db exists, it takes precedence
        fs::write(path.join("hash_cache.db"), b"").unwrap();
        fs::write(path.join("album_registry.json"), b"").unwrap();
        assert_eq!(determine_hash_cache_path(path), path.join("hash_cache.db"));
        assert_eq!(determine_album_registry_path(path), path.join("album_registry.json"));
    }

    #[test]
    fn test_go_yaml_config_with_string_last_update_check() {
        let yaml_str = r#"
account:
  credentials:
    - "Email=test%40gmail.com&Token=secret_token"
  selected: "test@gmail.com"
preferences:
  proxy: ""
  use_quota: false
  saver: true
  recursive: true
  force_upload: false
  upload_threads: 4
  auto_check_updates: true
  last_update_check: "2026-11-08T12:34:56Z"
"#;
        let cfg: Config = serde_yaml::from_str(yaml_str).expect("Failed to deserialize Go config");
        assert_eq!(cfg.account.selected, "test@gmail.com");
        assert_eq!(cfg.preferences.upload_threads, 4);
        assert!(cfg.preferences.saver);
        assert_eq!(cfg.preferences.last_update_check, 0);
    }
}
