use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use prost::Message;
use thiserror::Error;

use crate::album::{AlbumApi, AlbumError};
use crate::auth::{TokenManager, GOOGLE_PHOTOS_PACKAGE};
use crate::net::{create_http_client, GlobalRateLimiter};
use crate::proto::lumen::*;

pub const DEFAULT_PHOTOS_DATA_BASE: &str = "https://photosdata-pa.googleapis.com/6439526531001121323";
pub const DEFAULT_PHOTOS_UPLOAD_BASE: &str = "https://photos.googleapis.com";
pub const DEFAULT_AUTH_BASE: &str = "https://android.googleapis.com";

pub const LIVE_PHOTO_RESULT_ITEM_MASK_BASE64: &str = "CnsKAggBEAEaAggBIgIIASoQCgIIARICCAEaAggBIgIIATICCAGCAQIIAYoBAggBmgECCAGiAQIIAaoBCggBKgIgATICCAHKAQIIAfIBBggBEgIIAfoBAggBggICCAGKAgQKAggBkgIGCAESAggBogICCAGqAgC6AgDCAgAQASpiCAESKggBEhIIARoGCAESAggBIgQIASIAKAEiBggBEgIQASoGCAESAggBMAE4ARoWCAESDAgBGgIIASICCAEoARoCCAEwASIKCAESBggBEgIIASoOCgQIATABEgYIARICCAE6AggBQgIIAUoKCAESAggBGgISAFoSEgIIARoCCAEiCAgBEgQIARACYgIIAXISEgIIARoCCAEiCAgBEgQIARACeggKAggBIgIIAYoBBAoCCAGSAQQIARABqgECCgA=";

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    #[error("Authentication error: {0}")]
    Auth(String),
    #[error("API request failed with HTTP {status}: {body}")]
    HttpStatus { status: u16, body: String },
    #[error("Protobuf error: {0}")]
    Protobuf(String),
    #[error("IO error: {0}")]
    Io(String),
    #[error("Invalid Scotty finalize token")]
    InvalidScottyToken,
    #[error("Album not found: {0}")]
    AlbumNotFound(String),
    #[error("Network error: {0}")]
    Network(String),
}

impl From<ApiError> for AlbumError {
    fn from(err: ApiError) -> Self {
        match err {
            ApiError::AlbumNotFound(k) => AlbumError::NotFound(k),
            ApiError::HttpStatus { status, body } if status == 404 || body.to_lowercase().contains("not found") => {
                AlbumError::NotFound(body)
            }
            _ => AlbumError::Api(err.to_string()),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ApiOptions {
    pub account: String,
    pub proxy: String,
    pub saver: bool,
    pub use_quota: bool,
    pub speed_limit_kbps: u32,
    pub custom_endpoint_base: Option<String>,
}

pub struct Api {
    pub model: String,
    pub make: String,
    pub android_api_version: i64,
    pub client_version_code: i64,
    pub user_agent: String,
    pub language: String,
    pub account_email: String,
    pub auth_data: String,
    pub saver: bool,
    pub use_quota: bool,
    client: reqwest::Client,
    rate_limiter: GlobalRateLimiter,
    token_manager: Option<TokenManager>,
    custom_endpoint_base: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ScottyFinalizeToken {
    pub raw: Vec<u8>,
}

impl ScottyFinalizeToken {
    pub fn parse(raw: &[u8]) -> Result<Self, ApiError> {
        // Validate that raw bytes contain field 1 (varint 2) and field 2 (bytes)
        let token = CommitToken::decode(raw).map_err(|e| ApiError::Protobuf(e.to_string()))?;
        if token.field1 != 2 || token.field2.is_empty() {
            return Err(ApiError::InvalidScottyToken);
        }
        Ok(Self { raw: raw.to_vec() })
    }

    pub fn to_commit_token(&self) -> Result<CommitToken, ApiError> {
        CommitToken::decode(&self.raw[..]).map_err(|e| ApiError::Protobuf(e.to_string()))
    }
}

impl Api {
    pub fn new(opts: ApiOptions, credentials: &str) -> Result<Self, ApiError> {
        let client = create_http_client(if opts.proxy.is_empty() { None } else { Some(&opts.proxy) })
            .map_err(ApiError::Network)?;

        let mut email = opts.account.clone();
        if email.is_empty() {
            if let Some((_, e)) = url::form_urlencoded::parse(credentials.as_bytes()).find(|(k, _)| k == "Email") {
                email = e.into_owned();
            }
        }

        let mut api = Self {
            model: "Pixel XL".to_string(),
            make: "Google".to_string(),
            android_api_version: 28,
            client_version_code: 49029607,
            user_agent: String::new(),
            language: "en_US".to_string(),
            account_email: email,
            auth_data: credentials.trim().to_string(),
            saver: opts.saver,
            use_quota: opts.use_quota,
            client,
            rate_limiter: GlobalRateLimiter::new((opts.speed_limit_kbps as u64) * 1024),
            token_manager: None,
            custom_endpoint_base: opts.custom_endpoint_base,
        };

        api.apply_device_profile();
        Ok(api)
    }

    pub fn set_token_manager(&mut self, tm: TokenManager) {
        self.token_manager = Some(tm);
    }

    fn apply_device_profile(&mut self) {
        if self.use_quota {
            self.model = "Pixel 8".to_string();
            self.android_api_version = 34;
            self.user_agent = format!(
                "com.google.android.apps.photos/{} (Linux; U; Android 14; {}; {}; Build/UD1A.230803.041; Cronet/127.0.6510.5) (gzip)",
                self.client_version_code, self.language, self.model
            );
        } else if self.saver {
            self.model = "Pixel 2".to_string();
            self.android_api_version = 28;
            self.user_agent = format!(
                "com.google.android.apps.photos/{} (Linux; U; Android 9; {}; {}; Build/PQ2A.190205.001; Cronet/127.0.6510.5) (gzip)",
                self.client_version_code, self.language, self.model
            );
        } else {
            self.model = "Pixel XL".to_string();
            self.android_api_version = 28;
            self.user_agent = format!(
                "com.google.android.apps.photos/{} (Linux; U; Android 9; {}; {}; Build/PQ2A.190205.001; Cronet/127.0.6510.5) (gzip)",
                self.client_version_code, self.language, self.model
            );
        }
    }

    pub async fn bearer_token(&self) -> Result<String, ApiError> {
        if let Some(ref tm) = self.token_manager {
            if let Some(cached) = tm.get_cached_token() {
                return Ok(cached);
            }
        }

        // Fetch fresh bearer token
        let token_url = match &self.custom_endpoint_base {
            Some(base) => format!("{}/auth", base.trim_end_matches('/')),
            None => format!("{}/auth", DEFAULT_AUTH_BASE),
        };

        let mut form_params: Vec<(String, String)> = url::form_urlencoded::parse(self.auth_data.as_bytes())
            .into_owned()
            .collect();
        form_params.retain(|(k, _)| k != "it_caveat_types" && k != "assertion_jwt");
        form_params.push(("app".to_string(), GOOGLE_PHOTOS_PACKAGE.to_string()));
        form_params.push(("callerPkg".to_string(), GOOGLE_PHOTOS_PACKAGE.to_string()));

        let device_id = form_params
            .iter()
            .find(|(k, _)| k == "androidId")
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| "1234567890abcdef".to_string());

        let resp = self
            .client
            .post(&token_url)
            .header("Accept-Encoding", "gzip")
            .header("app", GOOGLE_PHOTOS_PACKAGE)
            .header("Connection", "Keep-Alive")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("device", device_id)
            .header("User-Agent", "GoogleAuth/1.4 (Pixel XL PQ2A.190205.001); gzip")
            .form(&form_params)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        let status = resp.status();
        let body = resp.text().await.map_err(|e| ApiError::Network(e.to_string()))?;

        if !status.is_success() {
            return Err(ApiError::HttpStatus {
                status: status.as_u16(),
                body,
            });
        }

        let parsed = crate::auth::parse_key_value_response(&body);
        let token = parsed
            .get("Auth")
            .ok_or_else(|| ApiError::Auth("auth response missing Auth token".to_string()))?
            .clone();

        let expiry_sec: i64 = parsed
            .get("Expiry")
            .and_then(|s| s.parse().ok())
            .unwrap_or(3600);

        if let Some(ref tm) = self.token_manager {
            tm.set_token(token.clone(), Duration::from_secs(std::cmp::max(60, expiry_sec as u64)));
        }

        Ok(token)
    }

    pub async fn find_remote_media_by_hash(&self, sha1_hash: &[u8]) -> Result<Option<String>, ApiError> {
        let endpoint = match &self.custom_endpoint_base {
            Some(base) => format!("{}/5084965799730810217", base.trim_end_matches('/')),
            None => format!("{}/5084965799730810217", DEFAULT_PHOTOS_DATA_BASE),
        };

        let req_proto = HashCheck {
            field1: Some(hash_check::Field1Type {
                field1: Some(hash_check::field1_type::Field1Type {
                    sha1_hash: sha1_hash.to_vec(),
                }),
                field2: Some(hash_check::field1_type::Field2Type {}),
            }),
        };

        let mut req_body = Vec::new();
        req_proto.encode(&mut req_body).map_err(|e| ApiError::Protobuf(e.to_string()))?;

        let bearer = self.bearer_token().await?;

        let resp = self
            .client
            .post(&endpoint)
            .header("Accept-Encoding", "gzip")
            .header("Accept-Language", &self.language)
            .header("Content-Type", "application/x-protobuf")
            .header("User-Agent", &self.user_agent)
            .header("Authorization", format!("Bearer {}", bearer))
            .body(req_body)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        let status = resp.status();
        let body_bytes = resp.bytes().await.map_err(|e| ApiError::Network(e.to_string()))?;

        if !status.is_success() {
            return Err(ApiError::HttpStatus {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&body_bytes).to_string(),
            });
        }

        if let Ok(matches) = RemoteMatches::decode(&body_bytes[..]) {
            if let Some(f1) = matches.field1 {
                if let Some(f2) = f1.field2 {
                    if let Some(inner) = f2.field2 {
                        if !inner.media_key.is_empty() {
                            return Ok(Some(inner.media_key));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    pub async fn get_upload_token(&self, sha1_b64: &str, file_size: i64) -> Result<String, ApiError> {
        let endpoint = match &self.custom_endpoint_base {
            Some(base) => format!("{}/data/upload/uploadmedia/interactive", base.trim_end_matches('/')),
            None => format!("{}/data/upload/uploadmedia/interactive", DEFAULT_PHOTOS_UPLOAD_BASE),
        };

        let req_proto = GetUploadToken {
            f1: 2,
            f2: 2,
            f3: 1,
            f4: 3,
            file_size_bytes: file_size,
        };

        let mut req_body = Vec::new();
        req_proto.encode(&mut req_body).map_err(|e| ApiError::Protobuf(e.to_string()))?;

        let bearer = self.bearer_token().await?;

        let resp = self
            .client
            .post(&endpoint)
            .header("Accept-Encoding", "gzip")
            .header("Accept-Language", &self.language)
            .header("Content-Type", "application/x-protobuf")
            .header("User-Agent", &self.user_agent)
            .header("Authorization", format!("Bearer {}", bearer))
            .header("X-Goog-Hash", format!("sha1={}", sha1_b64))
            .header("X-Upload-Content-Length", file_size.to_string())
            .body(req_body)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(ApiError::HttpStatus {
                status: status.as_u16(),
                body,
            });
        }

        let upload_id = resp
            .headers()
            .get("X-GUploader-UploadID")
            .or_else(|| resp.headers().get("x-guploader-uploadid"))
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiError::HttpStatus {
                status: status.as_u16(),
                body: "Missing X-GUploader-UploadID header".to_string(),
            })?
            .to_string();

        Ok(upload_id)
    }

    pub async fn upload_file_stream<F>(
        &self,
        upload_token: &str,
        file_path: &Path,
        mut on_progress: F,
    ) -> Result<ScottyFinalizeToken, ApiError>
    where
        F: FnMut(i64, i64, usize) + Send + 'static,
    {
        let metadata = std::fs::metadata(file_path).map_err(|e| ApiError::Io(e.to_string()))?;
        let file_size = metadata.len() as i64;

        let upload_url = match &self.custom_endpoint_base {
            Some(base) => format!("{}/data/upload/uploadmedia/interactive?upload_id={}", base.trim_end_matches('/'), upload_token),
            None => format!("{}/data/upload/uploadmedia/interactive?upload_id={}", DEFAULT_PHOTOS_UPLOAD_BASE, upload_token),
        };

        let bearer = self.bearer_token().await?;

        let file = tokio::fs::File::open(file_path).await.map_err(|e| ApiError::Io(e.to_string()))?;
        let chunk_size = 64 * 1024;
        let rate_limiter = self.rate_limiter.clone();
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<i64>();

        let progress_handle = tokio::spawn(async move {
            while let Some(uploaded) = progress_rx.recv().await {
                on_progress(uploaded, file_size, 1);
            }
        });

        let stream = futures_util::stream::unfold((file, 0i64), move |(mut file, mut uploaded)| {
            let rate_limiter = rate_limiter.clone();
            let progress_tx = progress_tx.clone();
            async move {
                use tokio::io::AsyncReadExt;
                let mut buf = vec![0u8; chunk_size];
                match file.read(&mut buf).await {
                    Ok(0) => None,
                    Ok(n) => {
                        buf.truncate(n);
                        rate_limiter.wait(n).await;
                        uploaded += n as i64;
                        let _ = progress_tx.send(uploaded);
                        Some((Ok::<bytes::Bytes, std::io::Error>(bytes::Bytes::from(buf)), (file, uploaded)))
                    }
                    Err(e) => Some((Err(e), (file, uploaded))),
                }
            }
        });

        let body = reqwest::Body::wrap_stream(stream);

        let resp = self
            .client
            .put(&upload_url)
            .header("Accept-Encoding", "gzip")
            .header("Accept-Language", &self.language)
            .header("User-Agent", &self.user_agent)
            .header("Authorization", format!("Bearer {}", bearer))
            .header("Content-Length", file_size.to_string())
            .body(body)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        let _ = progress_handle.await;

        let status = resp.status();
        let body_bytes = resp.bytes().await.map_err(|e| ApiError::Network(e.to_string()))?;

        if !status.is_success() {
            return Err(ApiError::HttpStatus {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&body_bytes).to_string(),
            });
        }

        ScottyFinalizeToken::parse(&body_bytes)
    }

    pub async fn commit_upload(
        &self,
        token: &ScottyFinalizeToken,
        file_name: &str,
        sha1_hash: &[u8],
        timestamp: i64,
    ) -> Result<String, ApiError> {
        let endpoint = match &self.custom_endpoint_base {
            Some(base) => format!("{}/16538846908252377752", base.trim_end_matches('/')),
            None => format!("{}/16538846908252377752", DEFAULT_PHOTOS_DATA_BASE),
        };

        let commit_tok = token.to_commit_token()?;
        let quality_val = if self.saver && !self.use_quota { 1 } else { 3 };

        let req_proto = CommitUpload {
            field1: Some(commit_upload::Field1Type {
                field1: Some(commit_upload::field1_type::Field1Type {
                    field1: commit_tok.field1,
                    field2: commit_tok.field2,
                }),
                file_name: file_name.to_string(),
                sha1_hash: sha1_hash.to_vec(),
                field4: Some(commit_upload::field1_type::Field4Type {
                    file_last_modified_timestamp: timestamp,
                    field2: 46_000_000,
                }),
                quality: quality_val,
                field8: None,
                field10: 1,
                field17: 0,
            }),
            field2: Some(commit_upload::Field2Type {
                model: self.model.clone(),
                make: self.make.clone(),
                android_api_version: self.android_api_version,
            }),
            field3: vec![1, 3],
        };

        let mut req_body = Vec::new();
        req_proto.encode(&mut req_body).map_err(|e| ApiError::Protobuf(e.to_string()))?;

        let bearer = self.bearer_token().await?;

        let resp = self
            .client
            .post(&endpoint)
            .header("Accept-Encoding", "gzip")
            .header("Accept-Language", &self.language)
            .header("Content-Type", "application/x-protobuf")
            .header("User-Agent", &self.user_agent)
            .header("Authorization", format!("Bearer {}", bearer))
            .header("x-goog-ext-173412678-bin", "CgcIAhClARgC")
            .header("x-goog-ext-174067345-bin", "CgIIAg==")
            .body(req_body)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        let status = resp.status();
        let body_bytes = resp.bytes().await.map_err(|e| ApiError::Network(e.to_string()))?;

        if !status.is_success() {
            return Err(ApiError::HttpStatus {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&body_bytes).to_string(),
            });
        }

        // Try decoding as CreateMediaItemsResponse
        if let Ok(resp_proto) = CreateMediaItemsResponse::decode(&body_bytes[..]) {
            for item in resp_proto.item {
                if let Some(res) = item.result_item {
                    if !res.media_key.is_empty() {
                        return Ok(res.media_key);
                    }
                }
            }
        }

        // Try decoding as CommitUploadResponse
        if let Ok(resp_proto) = CommitUploadResponse::decode(&body_bytes[..]) {
            if let Some(f1) = resp_proto.field1 {
                if let Some(f3) = f1.field3 {
                    if !f3.media_key.is_empty() {
                        return Ok(f3.media_key);
                    }
                }
            }
        }

        Err(ApiError::HttpStatus {
            status: 200,
            body: "Upload rejected by API: media key is empty or missing".to_string(),
        })
    }

    pub async fn commit_live_photo(
        &self,
        photo_token: &ScottyFinalizeToken,
        video_token: &ScottyFinalizeToken,
        file_name: &str,
        photo_sha1: &[u8],
        video_sha1: &[u8],
        timestamp: i64,
    ) -> Result<String, ApiError> {
        let endpoint = match &self.custom_endpoint_base {
            Some(base) => format!("{}/16538846908252377752", base.trim_end_matches('/')),
            None => format!("{}/16538846908252377752", DEFAULT_PHOTOS_DATA_BASE),
        };

        use base64::Engine;
        let mask = base64::engine::general_purpose::STANDARD
            .decode(LIVE_PHOTO_RESULT_ITEM_MASK_BASE64)
            .unwrap_or_default();

        let storage_policy = if self.saver && !self.use_quota { 1 } else { 3 };

        let blueprint = MediaItemBlueprint {
            upload_token: photo_token.raw.clone(),
            file_name: file_name.to_string(),
            source_sha1: photo_sha1.to_vec(),
            filesystem_create_time: Some(UploadTimestamp {
                seconds: timestamp,
                nanoseconds: 0,
            }),
            filesystem_mod_time: Some(UploadTimestamp {
                seconds: timestamp,
                nanoseconds: 0,
            }),
            storage_policy,
            reconcile_info: None,
            upload_quality: 1,
            live_photo_info: Some(LivePhotoInfo {
                video_upload_token: video_token.raw.clone(),
                video_source_sha1: video_sha1.to_vec(),
            }),
        };

        let request = CreateMediaItemsRequest {
            blueprint_array: vec![blueprint],
            upload_device_info: Some(UploadDeviceInfo {
                model: self.model.clone(),
                make: self.make.clone(),
                android_api_version: self.android_api_version,
            }),
            result_item_mask: mask,
        };

        let mut req_body = Vec::new();
        request.encode(&mut req_body).map_err(|e| ApiError::Protobuf(e.to_string()))?;

        let bearer = self.bearer_token().await?;

        let resp = self
            .client
            .post(&endpoint)
            .header("Accept-Encoding", "gzip")
            .header("Accept-Language", &self.language)
            .header("Content-Type", "application/x-protobuf")
            .header("User-Agent", &self.user_agent)
            .header("Authorization", format!("Bearer {}", bearer))
            .header("x-goog-ext-173412678-bin", "CgcIAhClARgC")
            .header("x-goog-ext-174067345-bin", "CgIIAg==")
            .body(req_body)
            .send()
            .await
            .map_err(|e| ApiError::Network(e.to_string()))?;

        let status = resp.status();
        let body_bytes = resp.bytes().await.map_err(|e| ApiError::Network(e.to_string()))?;

        if !status.is_success() {
            return Err(ApiError::HttpStatus {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&body_bytes).to_string(),
            });
        }

        if let Ok(resp_proto) = CreateMediaItemsResponse::decode(&body_bytes[..]) {
            for item in resp_proto.item {
                if let Some(res) = item.result_item {
                    if !res.media_key.is_empty() {
                        return Ok(res.media_key);
                    }
                }
            }
        }

        Err(ApiError::HttpStatus {
            status: 200,
            body: "Live Photo media key not received".to_string(),
        })
    }
}

#[async_trait::async_trait]
impl AlbumApi for Api {
    async fn create_album(&self, album_name: &str, media_keys: &[String]) -> Result<String, AlbumError> {
        let endpoint = match &self.custom_endpoint_base {
            Some(base) => format!("{}/8386163679468898444", base.trim_end_matches('/')),
            None => format!("{}/8386163679468898444", DEFAULT_PHOTOS_DATA_BASE),
        };

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;

        let proto_media_keys = media_keys
            .iter()
            .map(|k| create_album::Field4Type {
                field1: Some(create_album::field4_type::Field1Type {
                    media_key: k.clone(),
                }),
            })
            .collect();

        let req_proto = CreateAlbum {
            album_name: album_name.to_string(),
            timestamp: now,
            field3: 1,
            media_keys: proto_media_keys,
            field6: Some(create_album::Field6Type {}),
            field7: Some(create_album::Field7Type { field1: 3 }),
            device_info: Some(create_album::Field8Type {
                model: self.model.clone(),
                make: self.make.clone(),
                android_api_version: self.android_api_version,
            }),
        };

        let mut req_body = Vec::new();
        req_proto.encode(&mut req_body).map_err(|e| AlbumError::Api(e.to_string()))?;

        let bearer = self.bearer_token().await.map_err(|e| AlbumError::Api(e.to_string()))?;

        let resp = self
            .client
            .post(&endpoint)
            .header("Accept-Encoding", "gzip")
            .header("Accept-Language", &self.language)
            .header("Content-Type", "application/x-protobuf")
            .header("User-Agent", &self.user_agent)
            .header("Authorization", format!("Bearer {}", bearer))
            .header("x-goog-ext-173412678-bin", "CgcIAhClARgC")
            .header("x-goog-ext-174067345-bin", "CgIIAg==")
            .body(req_body)
            .send()
            .await
            .map_err(|e| AlbumError::Api(e.to_string()))?;

        let status = resp.status();
        let body_bytes = resp.bytes().await.map_err(|e| AlbumError::Api(e.to_string()))?;

        if !status.is_success() {
            let body = String::from_utf8_lossy(&body_bytes).to_string();
            if status == reqwest::StatusCode::NOT_FOUND || body.to_lowercase().contains("not found") {
                return Err(AlbumError::NotFound(album_name.to_string()));
            }
            return Err(AlbumError::Api(format!("HTTP {}: {}", status.as_u16(), body)));
        }

        let resp_proto = CreateAlbumResponse::decode(&body_bytes[..])
            .map_err(|e| AlbumError::Api(e.to_string()))?;

        let key = resp_proto
            .field1
            .map(|f| f.album_media_key)
            .unwrap_or_default();

        if key.is_empty() {
            return Err(AlbumError::Api("create album failed: no album media key returned".to_string()));
        }

        Ok(key)
    }

    async fn add_media_to_album(&self, album_media_key: &str, media_keys: &[String]) -> Result<(), AlbumError> {
        let endpoint = match &self.custom_endpoint_base {
            Some(base) => format!("{}/484917746253879292", base.trim_end_matches('/')),
            None => format!("{}/484917746253879292", DEFAULT_PHOTOS_DATA_BASE),
        };

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;

        let req_proto = AddMediaToAlbum {
            media_keys: media_keys.to_vec(),
            album_media_key: album_media_key.to_string(),
            field5: Some(add_media_to_album::Field5Type { field1: 2 }),
            device_info: Some(add_media_to_album::Field6Type {
                model: self.model.clone(),
                make: self.make.clone(),
                android_api_version: self.android_api_version,
            }),
            timestamp: now,
        };

        let mut req_body = Vec::new();
        req_proto.encode(&mut req_body).map_err(|e| AlbumError::Api(e.to_string()))?;

        let bearer = self.bearer_token().await.map_err(|e| AlbumError::Api(e.to_string()))?;

        let resp = self
            .client
            .post(&endpoint)
            .header("Accept-Encoding", "gzip")
            .header("Accept-Language", &self.language)
            .header("Content-Type", "application/x-protobuf")
            .header("User-Agent", &self.user_agent)
            .header("Authorization", format!("Bearer {}", bearer))
            .header("x-goog-ext-173412678-bin", "CgcIAhClARgC")
            .header("x-goog-ext-174067345-bin", "CgIIAg==")
            .body(req_body)
            .send()
            .await
            .map_err(|e| AlbumError::Api(e.to_string()))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            if status == reqwest::StatusCode::NOT_FOUND || body.to_lowercase().contains("not found") {
                return Err(AlbumError::NotFound(album_media_key.to_string()));
            }
            return Err(AlbumError::Api(format!("HTTP {}: {}", status.as_u16(), body)));
        }

        Ok(())
    }
}
