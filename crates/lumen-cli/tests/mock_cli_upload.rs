//! Integration test running CLI upload against a Wiremock mock server with 3 files.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::tempdir;
use prost::Message;
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, query_param};

use lumen_core::config::ConfigManager;
use lumen_core::proto::lumen::*;

fn helper_commit_token_bytes() -> Vec<u8> {
    let t = CommitToken {
        field1: 2,
        field2: b"opaque-cli-scotty-token".to_vec(),
    };
    let mut b = Vec::new();
    t.encode(&mut b).unwrap();
    b
}

fn helper_commit_response_bytes(media_key: &str) -> Vec<u8> {
    let resp = CommitUploadResponse {
        field1: Some(commit_upload_response::Field1Type {
            field1: None,
            field2: 0,
            field3: Some(commit_upload_response::field1_type::Field3Type {
                media_key: media_key.to_string(),
                ..Default::default()
            }),
        }),
        ..Default::default()
    };
    let mut b = Vec::new();
    resp.encode(&mut b).unwrap();
    b
}

fn helper_empty_remote_matches_bytes() -> Vec<u8> {
    let m = RemoteMatches { field1: None };
    let mut b = Vec::new();
    m.encode(&mut b).unwrap();
    b
}

fn create_sample_file(dir: &Path, name: &str, content: &[u8]) -> PathBuf {
    let p = dir.join(name);
    fs::write(&p, content).unwrap();
    p
}

#[tokio::test]
async fn test_cli_upload_sample_directory_3_files() {
    let mock_server = MockServer::start().await;
    let base_url = mock_server.uri();

    let dir = tempdir().unwrap();
    let photos_dir = dir.path().join("upload_folder");
    fs::create_dir_all(&photos_dir).unwrap();

    let _p1 = create_sample_file(&photos_dir, "pic1.jpg", b"image data 111");
    let _p2 = create_sample_file(&photos_dir, "pic2.jpg", b"image data 222");
    let _p3 = create_sample_file(&photos_dir, "pic3.jpg", b"image data 333");

    let cfg_path = dir.path().join("lumen.config");
    let cfg_mgr = Arc::new(ConfigManager::new_with_mock_keyring(cfg_path.clone()));
    cfg_mgr.save_credential("cli-user@example.com", "androidId=99999&Email=cli-user@example.com&Token=cli_oauth_token").unwrap();

    // 1. Mock /auth
    Mock::given(method("POST"))
        .and(path("/auth"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Auth=cli_bearer_token\nExpiry=3600\n"))
        .mount(&mock_server)
        .await;

    // 2. Mock HashCheck
    Mock::given(method("POST"))
        .and(path("/5084965799730810217"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_empty_remote_matches_bytes()))
        .mount(&mock_server)
        .await;

    // 3. Mock GetUploadToken
    Mock::given(method("POST"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .respond_with(ResponseTemplate::new(200).insert_header("X-GUploader-UploadID", "cli_session_123"))
        .mount(&mock_server)
        .await;

    // 4. Mock Scotty stream PUT
    Mock::given(method("PUT"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .and(query_param("upload_id", "cli_session_123"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_commit_token_bytes()))
        .mount(&mock_server)
        .await;

    // 5. Mock Commit
    Mock::given(method("POST"))
        .and(path("/16538846908252377752"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_commit_response_bytes("uploaded_cli_key")))
        .mount(&mock_server)
        .await;

    // Execute upload logic
    let upload_args = lumen_cli::UploadArgs {
        paths: vec![photos_dir],
        recursive: false,
        force_upload: false,
        saver: false,
        threads: Some(1),
        use_quota: false,
        album: None,
        album_auto: false,
        pair_live_photos: false,
        skip_incomplete_live_photos: true,
        update_existing_photos_to_live: false,
        ignore_apple_metadata: false,
        delete_from_host: false,
        disable_unsupported_files_filter: false,
        set_date_from_filename: false,
        exclude_pattern: None,
        speed_limit: None,
        account: Some("cli-user@example.com".to_string()),
        endpoint_base: Some(base_url),
    };

    let result = lumen_cli::handle_upload(upload_args, cfg_mgr, &cfg_path).await;
    assert!(result.is_ok(), "CLI upload of 3 files must succeed: {:?}", result.err());
}
