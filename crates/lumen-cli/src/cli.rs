//! Lumen CLI
//!
//! A native command-line interface for uploading media to Google Photos.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

use lumen_core::album::AlbumRegistry;
use lumen_core::auth::{
    build_google_photos_credential, exchange_embedded_setup_token, generate_android_id,
    looks_like_auth_string, normalize_oauth_token, EMBEDDED_SETUP_AUTH_ENDPOINT,
};
use lumen_core::config::{
    determine_album_registry_path, determine_config_path, determine_hash_cache_path, ConfigManager,
};
use lumen_core::dedup::HashCache;
use lumen_core::keyring::KeyringStorage;
use lumen_core::livephoto::{classify_upload_work, LivePhotoClassificationOptions};
use lumen_core::net::create_http_client;
use lumen_core::scanner::{scan_directories_incremental, ScanOptions};
use lumen_core::upload::{UploadEngine, UploadEvent, UploadOptions};

#[derive(Parser, Debug)]
#[command(
    name = "lumen-cli",
    about = "Native Google Photos desktop uploader with Material Design 3 and Live Photos support",
    version
)]
pub struct Cli {
    /// Path to configuration file (overrides default config path)
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    /// Enable verbose diagnostic logging
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Upload files or directories to Google Photos
    Upload(UploadArgs),

    /// Manage account credentials
    Creds(CredsArgs),

    /// Manage local hash deduplication cache
    Cache(CacheArgs),
}

#[derive(Args, Debug, Clone)]
pub struct UploadArgs {
    /// Files or directories to upload
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,

    /// Recurse into subdirectories
    #[arg(short = 'r', long)]
    pub recursive: bool,

    /// Force re-uploading files even if already present in library or cache
    #[arg(short = 'f', long, alias = "force")]
    pub force_upload: bool,

    /// Use Google Photos Storage Saver quality (free high-quality compression)
    #[arg(short = 's', long)]
    pub saver: bool,

    /// Number of concurrent upload threads (default from config or 5)
    #[arg(short = 't', long)]
    pub threads: Option<usize>,

    /// Count against Google Drive/Photos storage quota
    #[arg(short = 'u', long)]
    pub use_quota: bool,

    /// Album name or existing album link/key to add uploaded items to
    #[arg(short = 'a', long)]
    pub album: Option<String>,

    /// Automatically create albums matching each media file's folder name
    #[arg(long)]
    pub album_auto: bool,

    /// Pair matching photos (.heic, .jpg) and videos (.mov) as Live Photos
    #[arg(short = 'p', long)]
    pub pair_live_photos: bool,

    /// Skip incomplete Live Photos missing their photo or video component
    #[arg(long, default_value_t = true)]
    pub skip_incomplete_live_photos: bool,

    /// Update existing still photos in Google Photos into Live Photos if video found
    #[arg(long)]
    pub update_existing_photos_to_live: bool,

    /// Pair Live Photos strictly by filename stem instead of Apple MakerNote metadata
    #[arg(long)]
    pub ignore_apple_metadata: bool,

    /// Delete local files from computer upon successful upload
    #[arg(short = 'd', long = "delete-from-host", alias = "delete")]
    pub delete_from_host: bool,

    /// Upload all files regardless of Google Photos supported formats
    #[arg(long = "disable-unsupported-files-filter", alias = "disable-filter")]
    pub disable_unsupported_files_filter: bool,

    /// Extract capture date from filename instead of file modification time
    #[arg(long = "set-date-from-filename", alias = "date-from-filename")]
    pub set_date_from_filename: bool,

    /// Regular expression pattern of filenames to exclude
    #[arg(short = 'e', long = "exclude-pattern", alias = "exclude")]
    pub exclude_pattern: Option<String>,

    /// Maximum upload bandwidth limit in KB/s (0 = unlimited)
    #[arg(long)]
    pub speed_limit: Option<u32>,

    /// Account email to use (overrides active selected account)
    #[arg(long)]
    pub account: Option<String>,

    /// Custom API endpoint base URL (used for mock server testing)
    #[arg(long, hide = true)]
    pub endpoint_base: Option<String>,
}

#[derive(Args, Debug)]
pub struct CredsArgs {
    #[command(subcommand)]
    pub command: CredsCommands,
}

#[derive(Subcommand, Debug)]
pub enum CredsCommands {
    /// Add account credentials (oauth_token or key-value credential pair)
    Add {
        /// OAuth token or raw credential string
        credential: String,
    },

    /// List all configured accounts
    List,

    /// Set the active account
    Set {
        /// Email of account to make active
        email: String,
    },

    /// Remove an account and its credentials
    Remove {
        /// Email of account to remove
        email: String,
    },
}

#[derive(Args, Debug)]
pub struct CacheArgs {
    #[command(subcommand)]
    pub command: CacheCommands,
}

#[derive(Subcommand, Debug)]
pub enum CacheCommands {
    /// Clear all deduplication cache entries
    Clear,

    /// Show deduplication cache entry count
    Stats,
}

pub async fn main_entry() -> Result<()> {
    let cli = Cli::parse();

    let log_level = if cli.verbose { "debug" } else { "info" };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(log_level))
        .try_init();

    let config_path = determine_config_path(cli.config.as_deref());
    let keyring = Arc::new(KeyringStorage::new_os());
    let config_manager = Arc::new(ConfigManager::new(config_path.clone(), keyring));

    match cli.command {
        Commands::Upload(args) => handle_upload(args, config_manager, &config_path).await,
        Commands::Creds(args) => handle_creds(args, config_manager).await,
        Commands::Cache(args) => handle_cache(args, &config_path),
    }
}

pub async fn handle_upload(
    args: UploadArgs,
    config_manager: Arc<ConfigManager>,
    config_path: &Path,
) -> Result<()> {
    let cfg = config_manager.get_config();

    // Determine account
    let selected_account = if let Some(ref acc) = args.account {
        acc.trim().to_string()
    } else if !cfg.account.selected.is_empty() {
        cfg.account.selected.clone()
    } else {
        bail!("No account selected. Use 'lumen-cli creds add <token>' or specify '--account <email>'.");
    };

    // Verify credential exists for account
    if let Err(e) = config_manager.get_credential_for_account(Some(&selected_account)) {
        bail!("No credentials found for account '{}': {}", selected_account, e);
    }

    println!("Starting upload for account: {}", selected_account);

    // Scan paths
    println!("Scanning files...");
    let scan_opts = ScanOptions {
        recursive: args.recursive || cfg.preferences.recursive,
        filter_unsupported_files: !(args.disable_unsupported_files_filter || cfg.preferences.disable_unsupported_files_filter),
        exclude_pattern: args.exclude_pattern.clone().unwrap_or_else(|| cfg.preferences.exclude_pattern.clone()),
        ..Default::default()
    };

    let files = scan_directories_incremental(&args.paths, &scan_opts, Arc::new(AtomicBool::new(false)), |_| {})
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    if files.is_empty() {
        println!("No media files found to upload.");
        return Ok(());
    }

    println!("Found {} candidate files.", files.len());

    // Live Photo classification
    let live_opts = LivePhotoClassificationOptions {
        enabled: args.pair_live_photos || cfg.preferences.pair_live_photos,
        skip_incomplete: args.skip_incomplete_live_photos && cfg.preferences.skip_incomplete_live_photos,
        ignore_apple_metadata: args.ignore_apple_metadata,
        default_account: selected_account.clone(),
    };

    let (work_items, preflight_warnings) = classify_upload_work(&files, &live_opts);

    for w in &preflight_warnings {
        eprintln!("Warning [{}]: {}", w.code, w.message);
    }

    if work_items.is_empty() {
        println!("No upload work items remaining after classification.");
        return Ok(());
    }

    println!("Queued {} items for upload.", work_items.len());

    // Deduplication Hash Cache setup
    let cache_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let db_path = determine_hash_cache_path(cache_dir);
    let hash_cache = match HashCache::open(&db_path) {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("Notice: local hash cache unavailable ({}), proceeding without it.", e);
            None
        }
    };

    // Album Registry setup
    let album_reg_path = determine_album_registry_path(cache_dir);
    let album_registry = AlbumRegistry::new(album_reg_path)
        .unwrap_or_else(|_| AlbumRegistry::in_memory());

    // Upload options
    let threads = args.threads.unwrap_or(cfg.preferences.upload_threads);
    let speed_limit = args.speed_limit.unwrap_or(cfg.preferences.upload_speed_limit_kbps);
    let upload_opts = UploadOptions {
        threads,
        saver: args.saver || cfg.preferences.saver,
        use_quota: args.use_quota || cfg.preferences.use_quota,
        recursive: args.recursive || cfg.preferences.recursive,
        force_upload: args.force_upload || cfg.preferences.force_upload,
        delete_from_host: args.delete_from_host || cfg.preferences.delete_from_host,
        pair_live_photos: args.pair_live_photos || cfg.preferences.pair_live_photos,
        skip_incomplete_live_photos: args.skip_incomplete_live_photos && cfg.preferences.skip_incomplete_live_photos,
        update_existing_photos_to_live: args.update_existing_photos_to_live || cfg.preferences.update_existing_photos_to_live,
        ignore_apple_metadata: args.ignore_apple_metadata,
        set_date_from_filename: args.set_date_from_filename || cfg.preferences.set_date_from_filename,
        disable_unsupported_files_filter: args.disable_unsupported_files_filter || cfg.preferences.disable_unsupported_files_filter,
        exclude_pattern: args.exclude_pattern.unwrap_or(cfg.preferences.exclude_pattern),
        use_local_hash_cache: true,
        upload_speed_limit_kbps: speed_limit,
        album_name: args.album.unwrap_or_default(),
        album_auto_mode: args.album_auto,
        custom_endpoint_base: args.endpoint_base,
    };

    // Terminal progress with indicatif
    let mp = MultiProgress::new();
    let total_bar = mp.add(ProgressBar::new(work_items.len() as u64));
    total_bar.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} items ({percent}%) {msg}")
            .unwrap()
            .progress_chars("#>-"),
    );
    total_bar.set_message("Uploading...");

    let engine = UploadEngine::new(config_manager, hash_cache, album_registry);
    let (event_tx, mut event_rx) = mpsc::channel(100);

    let total_items = work_items.len();
    let mp_handle = tokio::spawn(async move {
        let mut thread_bars: HashMap<usize, ProgressBar> = HashMap::new();

        while let Some(event) = event_rx.recv().await {
            match event {
                UploadEvent::ThreadProgress(st) => {
                    let bar = thread_bars.entry(st.worker_id).or_insert_with(|| {
                        let b = mp.add(ProgressBar::new(100));
                        b.set_style(
                            ProgressStyle::default_bar()
                                .template("  Thread {prefix}: [{bar:20.yellow/dim}] {msg}")
                                .unwrap()
                                .progress_chars("=> "),
                        );
                        b.set_prefix(format!("{}", st.worker_id));
                        b
                    });

                    if st.bytes_total > 0 {
                        let pct = (st.bytes_uploaded * 100) / st.bytes_total;
                        bar.set_position(pct as u64);
                    }
                    bar.set_message(format!("{}: {}", st.status, st.file_name));
                }
                UploadEvent::FileCompleted(_res) => {
                    total_bar.inc(1);
                }
                UploadEvent::AccountBlocked { email, reason, .. } => {
                    total_bar.set_message(format!("Account {} BLOCKED ({})", email, reason));
                }
                UploadEvent::BatchComplete { .. } => {
                    break;
                }
                _ => {}
            }
        }

        for (_, b) in thread_bars {
            b.finish_and_clear();
        }
        total_bar.finish_with_message("Batch complete");
    });

    let results = engine.run_upload(work_items, upload_opts, event_tx).await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let _ = mp_handle.await;

    // Print summary
    let mut uploaded = 0;
    let mut skipped = 0;
    let mut failed = 0;

    for r in &results {
        if r.is_error {
            failed += 1;
            eprintln!("FAILED: {} - {}", r.path.display(), r.error_message);
        } else if r.skipped {
            skipped += 1;
            println!("SKIPPED [{}]: {}", r.skip_code, r.path.display());
        } else {
            uploaded += 1;
            println!("UPLOADED: {} (key: {})", r.path.display(), r.media_key);
        }
    }

    println!("\n=========================================");
    println!("Upload Summary");
    println!("Total Items: {}", total_items);
    println!("Uploaded:    {}", uploaded);
    println!("Skipped:     {}", skipped);
    println!("Failed:      {}", failed);
    println!("=========================================");

    if failed > 0 {
        bail!("{} upload(s) failed.", failed);
    }

    Ok(())
}

pub async fn handle_creds(args: CredsArgs, config_manager: Arc<ConfigManager>) -> Result<()> {
    match args.command {
        CredsCommands::Add { credential } => {
            let cred_input = if credential.trim() == "-" {
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf).context("Failed to read credential from standard input")?;
                buf
            } else {
                credential
            };
            let cred_str = cred_input.trim();
            if cred_str.is_empty() {
                bail!("Credential string cannot be empty.");
            }

            let (email, full_credential) = if looks_like_auth_string(cred_str) {
                let params: HashMap<String, String> = url::form_urlencoded::parse(cred_str.as_bytes())
                    .into_owned()
                    .collect();
                let email = params.get("Email").map(|s| s.trim()).unwrap_or("");
                let token = params.get("Token").map(|s| s.trim()).unwrap_or("");
                if email.is_empty() || email.eq_ignore_ascii_case("unknown@example.com") {
                    bail!("Invalid credential: Valid Google account Email is required in the auth string.");
                }
                if token.is_empty() {
                    bail!("Invalid credential: Token is required in the auth string.");
                }
                (email.to_string(), cred_str.to_string())
            } else {
                let clean_token = normalize_oauth_token(cred_str)
                    .map_err(|e| anyhow::anyhow!("Invalid oauth token: {}", e))?;

                let client = create_http_client(None)
                    .map_err(|e| anyhow::anyhow!("Failed to initialize HTTP client: {}", e))?;
                let android_id = generate_android_id();
                let exchange = exchange_embedded_setup_token(
                    &client,
                    EMBEDDED_SETUP_AUTH_ENDPOINT,
                    &clean_token,
                    &android_id,
                )
                .await
                .map_err(|e| anyhow::anyhow!("Token exchange failed: {}. Could not retrieve verified email.", e))?;

                if exchange.email.is_empty() || exchange.email.eq_ignore_ascii_case("unknown@example.com") {
                    bail!("Token exchange returned invalid or empty email.");
                }

                let cred = build_google_photos_credential(&exchange.email, &exchange.master_token, &android_id);
                (exchange.email, cred)
            };

            let save_status = config_manager.save_credential(&email, &full_credential)
                .map_err(|e| anyhow::anyhow!("Failed to save credential: {}", e))?;

            if !save_status.keyring_saved {
                println!("Warning: OS keyring unavailable; saving credentials directly to configuration file with mode 0600 permissions.");
            }

            println!("Successfully added credentials for account: {}", email);
            println!("Active account set to: {}", email);
        }
        CredsCommands::List => {
            let cfg = config_manager.get_config();
            let mut emails = Vec::new();
            for cred in &cfg.account.credentials {
                if let Some((_, email)) = url::form_urlencoded::parse(cred.as_bytes()).find(|(k, _)| k == "Email") {
                    emails.push(email.into_owned());
                }
            }

            if emails.is_empty() {
                println!("No accounts configured. Use 'lumen-cli creds add <token>' to add one.");
                return Ok(());
            }

            println!("Configured Accounts ({}):", emails.len());
            for email in emails {
                let marker = if email == cfg.account.selected { "* (active)" } else { "  " };
                println!("{} {}", marker, email);
            }
        }
        CredsCommands::Set { email } => {
            config_manager.update(|c| c.account.selected = email.clone())
                .map_err(|e| anyhow::anyhow!("Failed to set active account: {}", e))?;
            println!("Active account set to: {}", email);
        }
        CredsCommands::Remove { email } => {
            config_manager.remove_credential(&email)
                .map_err(|e| anyhow::anyhow!("Failed to remove account: {}", e))?;
            println!("Successfully removed account: {}", email);
        }
    }
    Ok(())
}

pub fn handle_cache(args: CacheArgs, config_path: &Path) -> Result<()> {
    let cache_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let db_path = determine_hash_cache_path(cache_dir);

    let hash_cache = HashCache::open(&db_path)
        .context("Failed to open hash cache database")?;

    match args.command {
        CacheCommands::Clear => {
            hash_cache.clear(None).context("Failed to clear hash cache")?;
            println!("Deduplication hash cache cleared successfully.");
        }
        CacheCommands::Stats => {
            let total = hash_cache.count(None).unwrap_or(0);
            println!("Hash Cache Statistics:");
            println!("Database path: {}", db_path.display());
            println!("Total entries: {}", total);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parsing_upload_flags() {
        let args = vec![
            "lumen-cli",
            "upload",
            "/photos",
            "-r",
            "-s",
            "-t",
            "4",
            "-a",
            "Vacation 2026",
            "--speed-limit",
            "1024",
        ];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Commands::Upload(u) => {
                assert_eq!(u.paths, vec![PathBuf::from("/photos")]);
                assert!(u.recursive);
                assert!(u.saver);
                assert_eq!(u.threads, Some(4));
                assert_eq!(u.album.as_deref(), Some("Vacation 2026"));
                assert_eq!(u.speed_limit, Some(1024));
            }
            _ => panic!("expected Upload command"),
        }
    }

    #[test]
    fn test_cli_parsing_upload_flags_aliases() {
        let args = vec![
            "lumen-cli",
            "upload",
            "/photos",
            "-f",
            "-d",
            "-e",
            "@eaDir",
            "--disable-filter",
            "--date-from-filename",
        ];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Commands::Upload(u) => {
                assert!(u.force_upload);
                assert!(u.delete_from_host);
                assert_eq!(u.exclude_pattern.as_deref(), Some("@eaDir"));
                assert!(u.disable_unsupported_files_filter);
                assert!(u.set_date_from_filename);
            }
            _ => panic!("expected Upload command"),
        }
    }

    #[test]
    fn test_cli_parsing_creds_commands() {
        let args = vec!["lumen-cli", "creds", "list"];
        let cli = Cli::try_parse_from(args).unwrap();
        match cli.command {
            Commands::Creds(c) => match c.command {
                CredsCommands::List => {}
                _ => panic!("expected List"),
            },
            _ => panic!("expected Creds command"),
        }
    }

    #[tokio::test]
    async fn test_creds_add_rejects_unknown_email_or_invalid_auth_string() {
        let temp = tempfile::tempdir().unwrap();
        let cfg_path = temp.path().join("lumen.config");
        let cfg_mgr = Arc::new(ConfigManager::new_with_mock_keyring(cfg_path));

        // Test with unknown@example.com
        let args = CredsArgs {
            command: CredsCommands::Add {
                credential: "Email=unknown@example.com&Token=dummy123".to_string(),
            },
        };
        let res = handle_creds(args, cfg_mgr.clone()).await;
        assert!(res.is_err(), "Must reject unknown@example.com");
        let err = res.unwrap_err().to_string();
        assert!(err.contains("Valid Google account Email is required"));

        // Verify config never saved unknown@example.com
        assert_ne!(cfg_mgr.get_config().account.selected, "unknown@example.com");

        // Test with missing Email in auth string
        let args_no_email = CredsArgs {
            command: CredsCommands::Add {
                credential: "Token=dummy123".to_string(),
            },
        };
        let res_no_email = handle_creds(args_no_email, cfg_mgr.clone()).await;
        assert!(res_no_email.is_err(), "Must reject auth string without email");
    }

    #[tokio::test]
    async fn test_creds_add_fails_with_clear_error_on_invalid_oauth_token() {
        let temp = tempfile::tempdir().unwrap();
        let cfg_path = temp.path().join("lumen.config");
        let cfg_mgr = Arc::new(ConfigManager::new_with_mock_keyring(cfg_path));

        // Attempting to add an invalid/fake oauth token must fail token exchange with a clear error
        // and never save "unknown@example.com"
        let args = CredsArgs {
            command: CredsCommands::Add {
                credential: "oauth_token=fake_invalid_token_123456789".to_string(),
            },
        };
        let res = handle_creds(args, cfg_mgr.clone()).await;
        assert!(res.is_err(), "Token exchange must fail on fake token");
        let err = res.unwrap_err().to_string();
        assert!(err.contains("Token exchange failed") || err.contains("Could not retrieve verified email"));

        // Verify nothing was saved as unknown@example.com
        assert_ne!(cfg_mgr.get_config().account.selected, "unknown@example.com");
        assert!(cfg_mgr.get_config().account.credentials.is_empty());
    }
}
