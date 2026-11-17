//! Resume probe: does Google Photos' mobile upload endpoint accept
//! Scotty-style resumable commands (`query`, offset-based chunks)?
//!
//! Usage (needs a real account already added to Lumen):
//!
//!   cargo run -p lumen-core --example resume_probe -- <file> [--account you@gmail.com] [--chunk-kib 1024]
//!
//! <file> must be larger than two chunks (default chunk = 1 MiB, so > 2 MiB).
//!
//! Nothing is ever committed, so no media item is created. Each probe opens
//! its own upload session and leaves it to expire on the server.

use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use lumen_core::api::{Api, ApiOptions, ScottyFinalizeToken, DEFAULT_PHOTOS_UPLOAD_BASE};
use lumen_core::auth::{
    build_google_photos_credential, exchange_embedded_setup_token, generate_android_id,
    normalize_oauth_token, EMBEDDED_SETUP_AUTH_ENDPOINT,
};
use lumen_core::config::{determine_config_path, ConfigManager};
use lumen_core::keyring::KeyringStorage;
use lumen_core::net::create_http_client;
use reqwest::{Client, Method};
use sha1::{Digest, Sha1};

struct Resp {
    status: u16,
    goog: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Resp {
    fn hdr(&self, name: &str) -> Option<&str> {
        self.goog
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

struct Probe {
    http: Client,
    bearer: String,
    ua: String,
    lang: String,
}

impl Probe {
    async fn send(
        &self,
        label: &str,
        method: Method,
        url: &str,
        extra: &[(&str, String)],
        body: Vec<u8>,
    ) -> Resp {
        let mut req = self
            .http
            .request(method.clone(), url)
            .header("Authorization", format!("Bearer {}", self.bearer))
            .header("User-Agent", &self.ua)
            .header("Accept-Language", &self.lang)
            .header("X-Goog-Upload-Protocol", "resumable")
            .header("Content-Length", body.len().to_string());
        for (k, v) in extra {
            req = req.header(*k, v.as_str());
        }

        let resp = match req.body(body).send().await {
            Ok(r) => r,
            Err(e) => {
                println!("[{label}] {method} -> network error: {e}");
                return Resp { status: 0, goog: vec![], body: vec![] };
            }
        };

        let status = resp.status().as_u16();
        let goog: Vec<(String, String)> = resp
            .headers()
            .iter()
            .filter(|(k, _)| {
                let k = k.as_str();
                k.starts_with("x-goog") || k.starts_with("x-guploader")
            })
            .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("<binary>").to_string()))
            .collect();
        let body = resp.bytes().await.map(|b| b.to_vec()).unwrap_or_default();

        println!("[{label}] {method} -> HTTP {status}");
        for (k, v) in &goog {
            if k != "x-guploader-uploadid" {
                println!("    {k}: {v}");
            }
        }
        if !body.is_empty() {
            let snip = &body[..body.len().min(120)];
            println!("    body ({} bytes): {:?}", body.len(), String::from_utf8_lossy(snip));
        }
        Resp { status, goog, body }
    }
}

async fn resolve_credential(
    raw_cred: &str,
    http: &Client,
    cfg: &ConfigManager,
) -> anyhow::Result<(String, String)> {
    let mut token_val = raw_cred.trim().to_string();
    let mut email_hint = String::new();

    if raw_cred.contains("Token=") {
        let params: std::collections::HashMap<String, String> =
            url::form_urlencoded::parse(raw_cred.as_bytes())
                .into_owned()
                .collect();
        if let Some(t) = params.get("Token") {
            token_val = t.clone();
        }
        if let Some(e) = params.get("Email") {
            email_hint = e.clone();
        }
    }

    // 1. One-time EmbeddedSetup OAuth token: exchange it for master token
    if token_val.starts_with("oauth2_4/") || token_val.contains("oauth2_4/") {
        println!("Detected EmbeddedSetup OAuth token; exchanging with Google for master token...");
        let clean = normalize_oauth_token(&token_val)
            .map_err(|e| anyhow::anyhow!("Token normalize error: {e}"))?;
        let android_id = generate_android_id();
        let exchange = exchange_embedded_setup_token(
            http,
            EMBEDDED_SETUP_AUTH_ENDPOINT,
            &clean,
            &android_id,
        )
        .await
        .map_err(|e| anyhow::anyhow!("OAuth setup token exchange failed: {e}"))?;

        let email = if !exchange.email.is_empty() && !exchange.email.contains("example.com") {
            exchange.email
        } else if !email_hint.is_empty() {
            email_hint
        } else {
            exchange.email
        };

        let full = build_google_photos_credential(&email, &exchange.master_token, &android_id);
        println!("Successfully exchanged token for account: {email}");
        let _ = cfg.save_credential(&email, &full);
        return Ok((email, full));
    }

    // 2. Partial Email=...&Token=... without Google Photos service params
    if raw_cred.contains("Email=") && raw_cred.contains("Token=") && !raw_cred.contains("service=") {
        let params: std::collections::HashMap<String, String> =
            url::form_urlencoded::parse(raw_cred.as_bytes())
                .into_owned()
                .collect();
        let email = params.get("Email").cloned().unwrap_or(email_hint);
        let token = params.get("Token").cloned().unwrap_or(token_val);
        let android_id = params
            .get("androidId")
            .cloned()
            .unwrap_or_else(generate_android_id);
        let full = build_google_photos_credential(&email, &token, &android_id);
        let _ = cfg.save_credential(&email, &full);
        return Ok((email, full));
    }

    Ok((email_hint, raw_cred.to_string()))
}

async fn new_session(api: &Api, sha1_b64: &str, size: i64) -> anyhow::Result<String> {
    let id = api.get_upload_token(sha1_b64, size).await?;
    let enc: String = url::form_urlencoded::byte_serialize(id.as_bytes()).collect();
    Ok(format!(
        "{}/data/upload/uploadmedia/interactive?upload_id={}",
        DEFAULT_PHOTOS_UPLOAD_BASE, enc
    ))
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn yes_no(b: bool) -> &'static str {
    if b { "YES" } else { "no" }
}

fn ok(status: u16) -> bool {
    (200..300).contains(&status)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args
        .first()
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("usage: resume_probe <file> [--account EMAIL] [--chunk-kib N]"))?;
    let account = flag(&args, "--account");
    let auth_flag = flag(&args, "--auth");
    let chunk = flag(&args, "--chunk-kib").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1024) * 1024;

    let data = std::fs::read(&path)?;
    anyhow::ensure!(
        data.len() > chunk * 2,
        "file is {} bytes; it must be larger than two chunks ({} bytes)",
        data.len(),
        chunk * 2
    );
    let size = data.len() as i64;
    let sha1_b64 = STANDARD.encode(Sha1::digest(&data));

    let cred = if let Some(raw) = auth_flag {
        raw
    } else {
        let cfg = ConfigManager::new(determine_config_path(None), Arc::new(KeyringStorage::new_os()));
        cfg.get_credential_for_account(account.as_deref())
            .map_err(|e| anyhow::anyhow!(e))?
    };
    let cfg = ConfigManager::new(determine_config_path(None), Arc::new(KeyringStorage::new_os()));
    let http = create_http_client(None).map_err(|e| anyhow::anyhow!(e))?;
    let (resolved_email, full_cred) = resolve_credential(&cred, &http, &cfg).await?;

    let effective_account = if !account.as_deref().unwrap_or("").is_empty() {
        account.clone().unwrap()
    } else if !resolved_email.is_empty() {
        resolved_email
    } else {
        "photos-user".to_string()
    };

    let api = Api::new(
        ApiOptions { account: effective_account, ..Default::default() },
        &full_cred,
    )?;

    let probe = Probe {
        http,
        bearer: api.bearer_token().await?,
        ua: api.user_agent.clone(),
        lang: api.language.clone(),
    };

    let mut query_supported = false;
    let mut resume_ok = false;

    // 1. Does a bare `query` on a fresh session get a status back?
    println!("\n== 1. 'query' on a fresh session ==");
    for method in [Method::POST, Method::PUT] {
        let url = new_session(&api, &sha1_b64, size).await?;
        let r = probe
            .send("query", method, &url, &[("X-Goog-Upload-Command", "query".into())], vec![])
            .await;
        if r.hdr("x-goog-upload-status").is_some() {
            query_supported = true;
        }
    }

    // 2. Send one chunk, ask how much arrived, then resume from that offset.
    println!("\n== 2. partial upload, query, resume ==");
    for method in [Method::POST, Method::PUT] {
        let url = new_session(&api, &sha1_b64, size).await?;

        let first = probe
            .send(
                "chunk 1",
                method.clone(),
                &url,
                &[
                    ("X-Goog-Upload-Command", "upload".into()),
                    ("X-Goog-Upload-Offset", "0".into()),
                ],
                data[..chunk].to_vec(),
            )
            .await;
        if !ok(first.status) {
            continue;
        }

        let q = probe
            .send("query", method.clone(), &url, &[("X-Goog-Upload-Command", "query".into())], vec![])
            .await;
        let received = q.hdr("x-goog-upload-size-received").and_then(|v| v.parse::<usize>().ok());

        match received {
            Some(n) if n > 0 && n <= data.len() => {
                query_supported = true;
                let fin = probe
                    .send(
                        "resume+finalize",
                        method.clone(),
                        &url,
                        &[
                            ("X-Goog-Upload-Command", "upload, finalize".into()),
                            ("X-Goog-Upload-Offset", n.to_string()),
                        ],
                        data[n..].to_vec(),
                    )
                    .await;
                if ok(fin.status) && ScottyFinalizeToken::parse(&fin.body).is_ok() {
                    println!("    finalize token parsed OK (nothing was committed)");
                    resume_ok = true;
                    break;
                }
            }
            _ => println!("    server did not report bytes received"),
        }
    }

    println!("\n== Verdict ==");
    println!("query command returns upload status / bytes received: {}", yes_no(query_supported));
    println!("partial upload resumed and finalized:                 {}", yes_no(resume_ok));
    if !query_supported && !resume_ok {
        println!("=> no sign of mid-file resume on this endpoint; stick to between-files pause.");
    }
    Ok(())
}
