use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use serde::{Deserialize, Serialize};

use crate::keyring::{KeyringStorage, MockKeyringProvider};

static CONFIG_TEMP_SEQ: AtomicU64 = AtomicU64::new(1);

fn secure_copy_file(src: &Path, dst: &Path) -> std::io::Result<()> {
    let meta = fs::symlink_metadata(src)?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Source path is not a regular file",
        ));
    }
    let bytes = fs::read(src)?;
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        opts.mode(0o600);
        opts.custom_flags(libc::O_NOFOLLOW);
        let mut f = opts.open(dst)?;
        let _ = f.set_permissions(fs::Permissions::from_mode(0o600));
        use std::io::Write;
        f.write_all(&bytes)?;
        f.sync_all()?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        use std::io::Write;
        let mut f = opts.open(dst)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        Ok(())
    }
}

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
    #[serde(default = "default_true", rename = "use_local_hash_cache")]
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
    #[serde(default = "default_auto_update_interval_hours", rename = "auto_update_interval_hours")]
    pub auto_update_interval_hours: u32,
    #[serde(default, rename = "last_update_check", deserialize_with = "deserialize_last_update_check")]
    pub last_update_check: i64,
    #[serde(default, rename = "schedule_enabled")]
    pub schedule_enabled: bool,
    #[serde(default = "default_schedule_start", rename = "schedule_start_hour")]
    pub schedule_start_hour: u8,
    #[serde(default = "default_schedule_end", rename = "schedule_end_hour")]
    pub schedule_end_hour: u8,
    #[serde(default, rename = "pause_on_battery")]
    pub pause_on_battery: bool,
    #[serde(default, rename = "dry_run")]
    pub dry_run: bool,
    #[serde(default = "default_locale", rename = "locale")]
    pub locale: String,
    #[serde(default, skip_serializing)]
    pub album_name: String,
    #[serde(default, skip_serializing)]
    pub album_auto_mode: bool,
}

fn default_locale() -> String {
    "en".to_string()
}

fn default_auto_update_interval_hours() -> u32 {
    6
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

fn default_schedule_start() -> u8 {
    1
}

fn default_schedule_end() -> u8 {
    7
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
            use_local_hash_cache: true,
            upload_speed_limit_kbps: 0,
            watch_folders_enabled: false,
            watched_folders: Vec::new(),
            watched_folder_configs: Vec::new(),
            recent_albums: Vec::new(),
            auto_check_updates: true,
            auto_update_interval_hours: default_auto_update_interval_hours(),
            last_update_check: 0,
            schedule_enabled: false,
            schedule_start_hour: 1,
            schedule_end_hour: 7,
            pause_on_battery: false,
            dry_run: false,
            locale: default_locale(),
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
        let config = match Self::load_from_path(&config_path) {
            Ok(cfg) => cfg,
            Err(err) => {
                tracing::error!("Failed to load config from {:?}: {}", config_path, err);
                if config_path.is_file() {
                    let mut bak_os = config_path.as_os_str().to_os_string();
                    bak_os.push(".corrupt");
                    let bak_path = PathBuf::from(bak_os);
                    if !bak_path.exists() {
                        let _ = secure_copy_file(&config_path, &bak_path);
                    }
                }
                Config::default()
            }
        };
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
        let mut next_cfg = cfg.clone();
        f(&mut next_cfg);
        let path = self.config_path.read().unwrap().clone();
        Self::save_to_path(&next_cfg, &path)?;
        *cfg = next_cfg;
        Ok(())
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

        Err("Configuration file is corrupted or unrecognized YAML/JSON".to_string())
    }

    pub fn save_to_path(config: &Config, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
            }
        }

        let yaml = serde_yaml::to_string(config).map_err(|e| e.to_string())?;
        let seq = CONFIG_TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config.yaml");
        let temp_path = path.with_file_name(format!(".{}.tmp.{}.{}", file_name, std::process::id(), seq));

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

        if let Err(e) = fs::rename(&temp_path, path) {
            let _ = fs::remove_file(&temp_path);
            return Err(e.to_string());
        }

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
        let keyring_err = match self.keyring.get_credential(&email) {
            Ok(Some(cred)) if !cred.trim().is_empty() => return Ok(cred),
            Ok(_) => None,
            Err(e) => Some(e),
        };

        // 2. Fall back to config credentials list (only if it still contains a Token)
        let mut found_sanitized_without_token = false;
        let cfg = self.config.read().unwrap();
        for cred in &cfg.account.credentials {
            let params: std::collections::HashMap<String, String> =
                url::form_urlencoded::parse(cred.as_bytes()).into_owned().collect();
            if let Some(parsed_email) = params.get("Email") {
                if parsed_email.eq_ignore_ascii_case(&email) {
                    if params.get("Token").map(|t| !t.trim().is_empty()).unwrap_or(false) {
                        return Ok(cred.clone());
                    }
                    found_sanitized_without_token = true;
                }
            }
        }

        if found_sanitized_without_token {
            if let Some(err) = keyring_err {
                return Err(format!(
                    "OS keyring unavailable for {}: {} (please unlock your keyring or sign in again)",
                    email, err
                ));
            }
            return Err(format!(
                "Keyring has no stored token for {} (session keyring may have been cleared); please sign in again",
                email
            ));
        }

        Err(format!("no credentials found for account: {}", email))
    }

    pub fn save_credential(&self, email: &str, credential: &str) -> Result<SaveCredentialResult, String> {
        let email = email.trim();
        if email.is_empty() {
            return Err("email cannot be empty".to_string());
        }

        // Try saving to keyring; if keyring storage fails, delete any stale entry
        // so an older keyring token cannot shadow the fallback credential on disk.
        let keyring_result = self.keyring.store_credential(email, credential);
        let keyring_saved = keyring_result.is_ok();
        let should_backup_in_file = !keyring_saved;
        if should_backup_in_file {
            let _ = self.keyring.delete_credential(email);
        }

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
        let _ = self.remove_credential_if_present(email)?;
        Ok(())
    }

    pub fn remove_credential_if_present(&self, email: &str) -> Result<bool, String> {
        let email = email.trim();
        let had_selected = {
            let cfg = self.config.read().unwrap();
            cfg.account.selected.eq_ignore_ascii_case(email)
                || cfg.account.credentials.iter().any(|c| {
                    url::form_urlencoded::parse(c.as_bytes())
                        .find(|(k, _)| k == "Email")
                        .map(|(_, e)| e.eq_ignore_ascii_case(email))
                        .unwrap_or(false)
                })
        };
        let keyring_existed = self.keyring.get_credential(email).is_ok();
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
        })?;

        Ok(had_selected || keyring_existed)
    }
}

pub const CONFIG_DIR_NAME: &str = if cfg!(debug_assertions) {
    "lumen-dev"
} else {
    "lumen"
};

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

    // 2. User config directory: separate dev (`lumen-dev`) and release (`lumen`) directories
    let user_config = get_user_config_dir();
    let lumen_dir = user_config.join(CONFIG_DIR_NAME);
    let lumen_config = lumen_dir.join("lumen.config");

    if lumen_config.exists() {
        return lumen_config;
    }

    // 3. Migration check: legacy gotohp directory (release channel only; never touch release/gotohp from dev builds)
    if !cfg!(debug_assertions) {
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
                    if secure_copy_file(&legacy_config, &lumen_config).is_ok() {
                        for (legacy_name, target_name) in [
                            ("hash_cache.db", "hash_cache.db"),
                            ("hashes.db", "hashes.db"),
                            ("album_registry.json", "album_registry.json"),
                            ("albums.json", "albums.json"),
                        ] {
                            let legacy_file = legacy_dir.join(legacy_name);
                            let target_file = lumen_dir.join(target_name);
                            if !target_file.exists() {
                                let _ = secure_copy_file(&legacy_file, &target_file);
                            }
                        }
                        return lumen_config;
                    }
                }
            }
            return legacy_config;
        }
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

    #[test]
    fn test_corrupt_config_returns_err_and_preserves_backup() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("lumen.config");
        fs::write(&path, ":::not valid yaml or json [[[").unwrap();

        assert!(ConfigManager::load_from_path(&path).is_err());

        let mgr = ConfigManager::new_with_mock_keyring(path.clone());
        let mut bak = path.as_os_str().to_os_string();
        bak.push(".corrupt");
        let bak_path = PathBuf::from(bak);
        assert!(bak_path.exists(), "corrupt config backup should be preserved");
        assert_eq!(mgr.get_config().account.selected, "");
    }

    #[test]
    fn test_get_credential_rejects_stripped_tokenless_fallback_when_keyring_empty() {
        use crate::keyring::{KeyringStorage, MockKeyringProvider};

        let dir = tempdir().unwrap();
        let path = dir.path().join("lumen.config");
        let keyring = Arc::new(KeyringStorage::new(Arc::new(MockKeyringProvider::new())));
        let mgr = ConfigManager::new(path, keyring.clone());

        let email = "lost@example.com";
        let cred = "androidId=9999&Email=lost@example.com&Token=secret_master_token";
        mgr.save_credential(email, cred).unwrap();

        // Simulate OS session keyring cleared (e.g. reboot on keyutils)
        keyring.delete_credential(email).unwrap();

        let err = mgr.get_credential_for_account(Some(email)).unwrap_err();
        assert!(
            err.contains("Keyring has no stored token") || err.contains("OS keyring unavailable"),
            "unexpected error: {}",
            err
        );
    }
}
