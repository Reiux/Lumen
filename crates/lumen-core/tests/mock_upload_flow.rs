//! Integration tests using wiremock to test all 5 core upload scenarios.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::tempdir;
use prost::Message;
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, header, headers, query_param};

use lumen_core::album::AlbumRegistry;
use lumen_core::config::ConfigManager;
use lumen_core::dedup::HashCache;
use lumen_core::livephoto::UploadWorkItem;
use lumen_core::proto::lumen::*;
use lumen_core::upload::{UploadEngine, UploadOptions, UploadEvent};

fn helper_commit_token_bytes() -> Vec<u8> {
    let t = CommitToken {
        field1: 2,
        field2: b"mock-scotty-token-data".to_vec(),
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

fn helper_matched_remote_matches_bytes(media_key: &str) -> Vec<u8> {
    let m = RemoteMatches {
        field1: Some(remote_matches::Field1Type {
            field2: Some(remote_matches::field1_type::Field2Type {
                field1: None,
                field2: Some(remote_matches::field1_type::field2_type::Field2Type {
                    media_key: media_key.to_string(),
                    field6: None,
                }),
            }),
        }),
    };
    let mut b = Vec::new();
    m.encode(&mut b).unwrap();
    b
}

fn helper_create_album_response_bytes(album_key: &str) -> Vec<u8> {
    let resp = CreateAlbumResponse {
        field1: Some(create_album_response::Field1Type {
            album_media_key: album_key.to_string(),
        }),
        field2: vec![],
        field3: None,
    };
    let mut b = Vec::new();
    resp.encode(&mut b).unwrap();
    b
}

fn create_test_file(dir: &Path, name: &str, content: &[u8]) -> PathBuf {
    let p = dir.join(name);
    fs::write(&p, content).unwrap();
    p
}

/// Scenario 1: Normal single upload flow
/// (HashCheck -> GetUploadToken -> upload bytes -> CommitToken -> AddMediaToAlbum)
#[tokio::test]
async fn test_scenario_1_normal_single_upload_flow() {
    let mock_server = MockServer::start().await;
    let base_url = mock_server.uri();

    let dir = tempdir().unwrap();
    let photo_path = create_test_file(dir.path(), "photo.jpg", b"sample photo bytes 1234567890");

    let cfg_path = dir.path().join("lumen.config");
    let cfg_mgr = Arc::new(ConfigManager::new_with_mock_keyring(cfg_path));
    cfg_mgr.save_credential("user@example.com", "androidId=12345&Email=user@example.com&Token=oauth2_token").unwrap();

    let db_path = dir.path().join("hashes.db");
    let hash_cache = HashCache::open(&db_path).unwrap();

    let album_reg_path = dir.path().join("albums.json");
    let album_registry = AlbumRegistry::new(album_reg_path).unwrap();

    // 1. Mock /auth
    Mock::given(method("POST"))
        .and(path("/auth"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Auth=bearer_token_user_1\nExpiry=3600\n"))
        .mount(&mock_server)
        .await;

    // 2. Mock HashCheck (empty matches)
    Mock::given(method("POST"))
        .and(path("/5084965799730810217"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_empty_remote_matches_bytes()))
        .mount(&mock_server)
        .await;

    // 3. Mock GetUploadToken
    Mock::given(method("POST"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("X-GUploader-UploadID", "mock_upload_session_1")
        )
        .mount(&mock_server)
        .await;

    // 4. Mock Scotty stream PUT
    Mock::given(method("PUT"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .and(query_param("upload_id", "mock_upload_session_1"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_commit_token_bytes()))
        .mount(&mock_server)
        .await;

    // 5. Mock Commit
    Mock::given(method("POST"))
        .and(path("/16538846908252377752"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_commit_response_bytes("photo_media_key_101")))
        .mount(&mock_server)
        .await;

    // 6. Mock CreateAlbum
    Mock::given(method("POST"))
        .and(path("/8386163679468898444"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_create_album_response_bytes("album_key_101")))
        .mount(&mock_server)
        .await;

    let engine = UploadEngine::new(cfg_mgr, Some(hash_cache.clone()), album_registry);
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(100);

    let opts = UploadOptions {
        threads: 1,
        album_name: "Vacation".to_string(),
        custom_endpoint_base: Some(base_url),
        ..Default::default()
    };

    let items = vec![UploadWorkItem::Single {
        path: photo_path.clone(),
        account_email: "user@example.com".to_string(),
    }];

    // Drain events in background
    let drain_handle = tokio::spawn(async move {
        while let Some(_event) = event_rx.recv().await {}
    });

    let results = engine.run_upload(items, opts, event_tx).await.unwrap();
    let _ = drain_handle.await;

    assert_eq!(results.len(), 1);
    let res = &results[0];
    assert!(!res.is_error, "upload should succeed: {}", res.error_message);
    assert!(!res.skipped, "should not be skipped");
    assert_eq!(res.media_key, "photo_media_key_101");
    assert_eq!(res.account_email, "user@example.com");

    // Verify hash cache was updated
    let (_raw, sha1_b64) = lumen_core::dedup::calculate_file_sha1(&photo_path).unwrap();
    let size = fs::metadata(&photo_path).unwrap().len() as i64;
    assert!(hash_cache.get("user@example.com", &sha1_b64, size).unwrap().is_some());
}

/// Scenario 2: Duplicate skip
/// (Part A: local cache hit skips immediately without network calls)
/// (Part B: remote match skips upload bytes and adds to album)
#[tokio::test]
async fn test_scenario_2_duplicate_skip_local_and_remote() {
    let mock_server = MockServer::start().await;
    let base_url = mock_server.uri();

    let dir = tempdir().unwrap();
    let photo1 = create_test_file(dir.path(), "cached.jpg", b"local cached image");
    let photo2 = create_test_file(dir.path(), "remote.jpg", b"remote match image");

    let cfg_path = dir.path().join("lumen.config");
    let cfg_mgr = Arc::new(ConfigManager::new_with_mock_keyring(cfg_path));
    cfg_mgr.save_credential("user@example.com", "androidId=12345&Email=user@example.com&Token=oauth2_token").unwrap();

    let db_path = dir.path().join("hashes.db");
    let hash_cache = HashCache::open(&db_path).unwrap();

    let album_reg_path = dir.path().join("albums.json");
    let album_registry = AlbumRegistry::new(album_reg_path).unwrap();

    // Pre-insert photo1 into local hash cache
    let (_raw, photo1_sha1) = lumen_core::dedup::calculate_file_sha1(&photo1).unwrap();
    let photo1_size = fs::metadata(&photo1).unwrap().len() as i64;
    hash_cache.put("user@example.com", &photo1_sha1, "existing_local_key", photo1_size, 1000).unwrap();

    // Mock /auth for photo2
    Mock::given(method("POST"))
        .and(path("/auth"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Auth=bearer_token_user_1\nExpiry=3600\n"))
        .mount(&mock_server)
        .await;

    // Mock HashCheck for photo2 (returns remote match)
    Mock::given(method("POST"))
        .and(path("/5084965799730810217"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_matched_remote_matches_bytes("remote_key_888")))
        .mount(&mock_server)
        .await;

    // Mock CreateAlbum for photo2
    Mock::given(method("POST"))
        .and(path("/8386163679468898444"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_create_album_response_bytes("album_key_101")))
        .mount(&mock_server)
        .await;

    let engine = UploadEngine::new(cfg_mgr, Some(hash_cache.clone()), album_registry);
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(100);

    let opts = UploadOptions {
        threads: 1,
        album_name: "Album".to_string(),
        custom_endpoint_base: Some(base_url),
        ..Default::default()
    };

    let items = vec![
        UploadWorkItem::Single {
            path: photo1.clone(),
            account_email: "user@example.com".to_string(),
        },
        UploadWorkItem::Single {
            path: photo2.clone(),
            account_email: "user@example.com".to_string(),
        },
    ];

    let drain_handle = tokio::spawn(async move {
        while let Some(_event) = event_rx.recv().await {}
    });

    let results = engine.run_upload(items, opts, event_tx).await.unwrap();
    let _ = drain_handle.await;

    assert_eq!(results.len(), 2);

    // Item 1: skipped via local dedup
    assert_eq!(results[0].skip_code, "hash-cache");
    assert!(results[0].skipped);
    assert!(!results[0].is_error);

    // Item 2: skipped via remote match (upload bytes skipped, album add succeeded)
    assert_eq!(results[1].skip_code, "remote-dedup");
    assert!(results[1].skipped);
    assert!(!results[1].is_error);
    assert_eq!(results[1].media_key, "remote_key_888");

    // Photo2 should now be in local hash cache
    let (_raw, photo2_sha1) = lumen_core::dedup::calculate_file_sha1(&photo2).unwrap();
    let photo2_size = fs::metadata(&photo2).unwrap().len() as i64;
    assert!(hash_cache.get("user@example.com", &photo2_sha1, photo2_size).unwrap().is_some());
}

/// Scenario 3: Auth failure (401/403) trips the circuit breaker for that account
/// and stops that worker immediately without blind retries.
#[tokio::test]
async fn test_scenario_3_auth_failure_trips_circuit_breaker() {
    let mock_server = MockServer::start().await;
    let base_url = mock_server.uri();

    let dir = tempdir().unwrap();
    let photo1 = create_test_file(dir.path(), "photo1.jpg", b"image 1");
    let photo2 = create_test_file(dir.path(), "photo2.jpg", b"image 2");

    let cfg_path = dir.path().join("lumen.config");
    let cfg_mgr = Arc::new(ConfigManager::new_with_mock_keyring(cfg_path));
    cfg_mgr.save_credential("user@example.com", "androidId=12345&Email=user@example.com&Token=bad_token").unwrap();

    let db_path = dir.path().join("hashes.db");
    let hash_cache = HashCache::open(&db_path).unwrap();

    let album_reg_path = dir.path().join("albums.json");
    let album_registry = AlbumRegistry::new(album_reg_path).unwrap();

    // Mock /auth returning 401 Unauthorized
    Mock::given(method("POST"))
        .and(path("/auth"))
        .respond_with(ResponseTemplate::new(401).set_body_string("Error=BadAuthentication\n"))
        .expect(1) // EXACTLY 1 attempt: NO BLIND RETRIES
        .mount(&mock_server)
        .await;

    let engine = UploadEngine::new(cfg_mgr, Some(hash_cache), album_registry);
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(100);

    let opts = UploadOptions {
        threads: 1,
        custom_endpoint_base: Some(base_url),
        ..Default::default()
    };

    let items = vec![
        UploadWorkItem::Single {
            path: photo1,
            account_email: "user@example.com".to_string(),
        },
        UploadWorkItem::Single {
            path: photo2,
            account_email: "user@example.com".to_string(),
        },
    ];

    let mut blocked_events = 0;
    let drain_handle = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            if let UploadEvent::AccountBlocked { email, .. } = event {
                if email == "user@example.com" {
                    blocked_events += 1;
                }
            }
        }
        blocked_events
    });

    let results = engine.run_upload(items, opts, event_tx).await.unwrap();
    let blocked_received = drain_handle.await.unwrap();

    assert_eq!(results.len(), 2);
    // Item 1 had the auth error
    assert!(results[0].is_error);
    // Item 2 was immediately blocked by the circuit breaker without calling /auth again!
    assert!(results[1].is_error);
    assert_eq!(results[1].skip_code, "account-blocked");
    assert!(blocked_received >= 1);
}

/// Scenario 4: Album 404 (album add returns 404 -> marked missing, error returned to caller,
/// photo is NOT silently dumped in the timeline without album association)
#[tokio::test]
async fn test_scenario_4_album_404_handling() {
    let mock_server = MockServer::start().await;
    let base_url = mock_server.uri();

    let dir = tempdir().unwrap();
    let photo = create_test_file(dir.path(), "album_test.jpg", b"sample photo bytes");

    let cfg_path = dir.path().join("lumen.config");
    let cfg_mgr = Arc::new(ConfigManager::new_with_mock_keyring(cfg_path));
    cfg_mgr.save_credential("user@example.com", "androidId=12345&Email=user@example.com&Token=oauth2_token").unwrap();

    let db_path = dir.path().join("hashes.db");
    let hash_cache = HashCache::open(&db_path).unwrap();

    let album_reg_path = dir.path().join("albums.json");
    let album_registry = AlbumRegistry::new(album_reg_path.clone()).unwrap();

    // Pre-insert an existing album entry to test that NotFound clears it and fails
    album_registry.set("user@example.com", "Missing Album", "stale_album_key").unwrap();

    // 1. Mock /auth
    Mock::given(method("POST"))
        .and(path("/auth"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Auth=bearer_token_user_1\nExpiry=3600\n"))
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
        .respond_with(ResponseTemplate::new(200).insert_header("X-GUploader-UploadID", "mock_upload_session_404"))
        .mount(&mock_server)
        .await;

    // 4. Mock Scotty stream PUT
    Mock::given(method("PUT"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_commit_token_bytes()))
        .mount(&mock_server)
        .await;

    // 5. Mock Commit
    Mock::given(method("POST"))
        .and(path("/16538846908252377752"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_commit_response_bytes("photo_media_key_404")))
        .mount(&mock_server)
        .await;

    // 6. Mock Add to Album: FAILS WITH 404 NOT FOUND (stale key)
    Mock::given(method("POST"))
        .and(path("/484917746253879292"))
        .respond_with(ResponseTemplate::new(404).set_body_string("Album not found"))
        .mount(&mock_server)
        .await;

    // 7. Mock CreateAlbum: ALSO FAILS WITH 404 to ensure error is bubbled
    Mock::given(method("POST"))
        .and(path("/8386163679468898444"))
        .respond_with(ResponseTemplate::new(404).set_body_string("Cannot create album"))
        .mount(&mock_server)
        .await;

    let engine = UploadEngine::new(cfg_mgr, Some(hash_cache), album_registry.clone());
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(100);

    let opts = UploadOptions {
        threads: 1,
        album_name: "Missing Album".to_string(),
        custom_endpoint_base: Some(base_url),
        ..Default::default()
    };

    let items = vec![UploadWorkItem::Single {
        path: photo,
        account_email: "user@example.com".to_string(),
    }];

    let drain_handle = tokio::spawn(async move {
        while let Some(_event) = event_rx.recv().await {}
    });

    let results = engine.run_upload(items, opts, event_tx).await.unwrap();
    let _ = drain_handle.await;

    assert_eq!(results.len(), 1);
    let res = &results[0];
    // Must return an error to caller, not silently consider it uploaded without album!
    assert!(res.is_error, "album 404 must result in an error result");
    assert!(res.error_message.to_lowercase().contains("album") || res.error_message.to_lowercase().contains("not found"));
    // The stale album key should have been evicted from registry
    assert!(album_registry.get("user@example.com", "Missing Album").is_none());
}

/// Scenario 5: Two-account queue where each item uses its own account's token
#[tokio::test]
async fn test_scenario_5_two_account_queue_isolation() {
    let mock_server = MockServer::start().await;
    let base_url = mock_server.uri();

    let dir = tempdir().unwrap();
    let photo_alice = create_test_file(dir.path(), "alice_pic.jpg", b"alice sample image");
    let photo_bob = create_test_file(dir.path(), "bob_pic.jpg", b"bob sample image");

    let cfg_path = dir.path().join("lumen.config");
    let cfg_mgr = Arc::new(ConfigManager::new_with_mock_keyring(cfg_path));
    cfg_mgr.save_credential("alice@example.com", "androidId=11111&Email=alice@example.com&Token=token_alice").unwrap();
    cfg_mgr.save_credential("bob@example.com", "androidId=22222&Email=bob@example.com&Token=token_bob").unwrap();

    let db_path = dir.path().join("hashes.db");
    let hash_cache = HashCache::open(&db_path).unwrap();

    let album_reg_path = dir.path().join("albums.json");
    let album_registry = AlbumRegistry::new(album_reg_path).unwrap();

    struct BodyContains(&'static str);
    impl wiremock::Match for BodyContains {
        fn matches(&self, request: &wiremock::Request) -> bool {
            String::from_utf8_lossy(&request.body).contains(self.0)
        }
    }

    // Mock /auth for Alice (matches alice's credentials in form)
    Mock::given(method("POST"))
        .and(path("/auth"))
        .and(BodyContains("alice"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Auth=alice_bearer_token\nExpiry=3600\n"))
        .mount(&mock_server)
        .await;

    // Mock /auth for Bob (matches bob's credentials in form)
    Mock::given(method("POST"))
        .and(path("/auth"))
        .and(BodyContains("bob"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Auth=bob_bearer_token\nExpiry=3600\n"))
        .mount(&mock_server)
        .await;

    // Mock HashCheck for Alice
    Mock::given(method("POST"))
        .and(path("/5084965799730810217"))
        .and(header("Authorization", "Bearer alice_bearer_token"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_empty_remote_matches_bytes()))
        .mount(&mock_server)
        .await;

    // Mock HashCheck for Bob
    Mock::given(method("POST"))
        .and(path("/5084965799730810217"))
        .and(header("Authorization", "Bearer bob_bearer_token"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_empty_remote_matches_bytes()))
        .mount(&mock_server)
        .await;

    // Mock GetUploadToken for Alice
    Mock::given(method("POST"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .and(header("Authorization", "Bearer alice_bearer_token"))
        .respond_with(ResponseTemplate::new(200).insert_header("X-GUploader-UploadID", "upload_session_alice"))
        .mount(&mock_server)
        .await;

    // Mock GetUploadToken for Bob
    Mock::given(method("POST"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .and(header("Authorization", "Bearer bob_bearer_token"))
        .respond_with(ResponseTemplate::new(200).insert_header("X-GUploader-UploadID", "upload_session_bob"))
        .mount(&mock_server)
        .await;

    // Mock PUT stream for Alice
    Mock::given(method("PUT"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .and(query_param("upload_id", "upload_session_alice"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_commit_token_bytes()))
        .mount(&mock_server)
        .await;

    // Mock PUT stream for Bob
    Mock::given(method("PUT"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .and(query_param("upload_id", "upload_session_bob"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_commit_token_bytes()))
        .mount(&mock_server)
        .await;

    // Mock Commit for Alice
    Mock::given(method("POST"))
        .and(path("/16538846908252377752"))
        .and(header("Authorization", "Bearer alice_bearer_token"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_commit_response_bytes("alice_key_101")))
        .mount(&mock_server)
        .await;

    // Mock Commit for Bob
    Mock::given(method("POST"))
        .and(path("/16538846908252377752"))
        .and(header("Authorization", "Bearer bob_bearer_token"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_commit_response_bytes("bob_key_202")))
        .mount(&mock_server)
        .await;

    let engine = UploadEngine::new(cfg_mgr, Some(hash_cache.clone()), album_registry);
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(100);

    let opts = UploadOptions {
        threads: 2,
        custom_endpoint_base: Some(base_url),
        ..Default::default()
    };

    let items = vec![
        UploadWorkItem::Single {
            path: photo_alice.clone(),
            account_email: "alice@example.com".to_string(),
        },
        UploadWorkItem::Single {
            path: photo_bob.clone(),
            account_email: "bob@example.com".to_string(),
        },
    ];

    let drain_handle = tokio::spawn(async move {
        while let Some(_event) = event_rx.recv().await {}
    });

    let results = engine.run_upload(items, opts, event_tx).await.unwrap();
    let _ = drain_handle.await;

    assert_eq!(results.len(), 2);

    let alice_res = results.iter().find(|r| r.account_email == "alice@example.com").unwrap();
    assert!(!alice_res.is_error, "alice upload error: {}", alice_res.error_message);
    assert_eq!(alice_res.media_key, "alice_key_101");

    let bob_res = results.iter().find(|r| r.account_email == "bob@example.com").unwrap();
    assert!(!bob_res.is_error, "bob upload error: {}", bob_res.error_message);
    assert_eq!(bob_res.media_key, "bob_key_202");

    // Each account's hash cache is isolated
    let (_raw, alice_sha1) = lumen_core::dedup::calculate_file_sha1(&photo_alice).unwrap();
    let alice_size = fs::metadata(&photo_alice).unwrap().len() as i64;
    assert!(hash_cache.get("alice@example.com", &alice_sha1, alice_size).unwrap().is_some());
    assert!(hash_cache.get("bob@example.com", &alice_sha1, alice_size).unwrap().is_none());

    let (_raw, bob_sha1) = lumen_core::dedup::calculate_file_sha1(&photo_bob).unwrap();
    let bob_size = fs::metadata(&photo_bob).unwrap().len() as i64;
    assert!(hash_cache.get("bob@example.com", &bob_sha1, bob_size).unwrap().is_some());
    assert!(hash_cache.get("alice@example.com", &bob_sha1, bob_size).unwrap().is_none());
}

#[tokio::test]
async fn test_scenario_6_delete_from_host_waits_for_album_attachment() {
    let mock_server = MockServer::start().await;
    let base_url = mock_server.uri();

    let dir = tempdir().unwrap();
    let photo = dir.path().join("delete_guard.jpg");
    fs::write(&photo, b"delete_guard_bytes").unwrap();

    let cfg_path = dir.path().join("config.yaml");
    let cfg_mgr = Arc::new(ConfigManager::new_with_mock_keyring(cfg_path));
    cfg_mgr
        .save_credential(
            "user@example.com",
            "androidId=12345&Email=user@example.com&Token=oauth2_token",
        )
        .unwrap();

    let db_path = dir.path().join("hashes.db");
    let hash_cache = HashCache::open(&db_path).unwrap();
    let album_registry = AlbumRegistry::new(dir.path().join("albums.json")).unwrap();

    Mock::given(method("POST"))
        .and(path("/auth"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Auth=bearer_tok\nExpiry=3600\n"))
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/5084965799730810217"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_empty_remote_matches_bytes()))
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .respond_with(ResponseTemplate::new(200).insert_header("X-GUploader-UploadID", "sess_del"))
        .mount(&mock_server)
        .await;

    Mock::given(method("PUT"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_commit_token_bytes()))
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/16538846908252377752"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_commit_response_bytes("media_key_del")))
        .mount(&mock_server)
        .await;

    // Album creation fails -> run_upload returns Err and must NOT delete local file
    Mock::given(method("POST"))
        .and(path("/8386163679468898444"))
        .respond_with(ResponseTemplate::new(500).set_body_string("album failure"))
        .mount(&mock_server)
        .await;

    let engine = UploadEngine::new(cfg_mgr, Some(hash_cache), album_registry);
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(100);
    let drain_handle = tokio::spawn(async move {
        while let Some(_event) = event_rx.recv().await {}
    });

    let opts = UploadOptions {
        threads: 1,
        album_name: "Fail Album".to_string(),
        delete_from_host: true,
        custom_endpoint_base: Some(base_url),
        ..Default::default()
    };

    let results = engine
        .run_upload(
            vec![UploadWorkItem::Single {
                path: photo.clone(),
                account_email: "user@example.com".to_string(),
            }],
            opts,
            event_tx,
        )
        .await
        .unwrap();
    let _ = drain_handle.await;

    assert_eq!(results.len(), 1);
    assert!(results[0].is_error, "album failure must mark item as error");
    assert!(photo.exists(), "local file must NOT be deleted when album attachment fails");
}

#[tokio::test]
async fn test_scenario_7_transient_retry_and_401_invalidation() {
    let mock_server = MockServer::start().await;
    let base_url = mock_server.uri();

    let dir = tempdir().unwrap();
    let photo = dir.path().join("retry_test.jpg");
    fs::write(&photo, b"retry_payload_bytes").unwrap();

    let cfg_path = dir.path().join("config.yaml");
    let cfg_mgr = Arc::new(ConfigManager::new_with_mock_keyring(cfg_path));
    cfg_mgr
        .save_credential(
            "user@example.com",
            "androidId=12345&Email=user@example.com&Token=oauth2_token",
        )
        .unwrap();

    let db_path = dir.path().join("hashes.db");
    let hash_cache = HashCache::open(&db_path).unwrap();
    let album_registry = AlbumRegistry::new(dir.path().join("albums.json")).unwrap();

    // First /auth returns tok1 (with epoch expiry far in future), second returns tok2 after 401 invalidates tok1
    Mock::given(method("POST"))
        .and(path("/auth"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Auth=tok1\nExpiry=9999999999\n"))
        .up_to_n_times(1)
        .mount(&mock_server)
        .await;
    Mock::given(method("POST"))
        .and(path("/auth"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Auth=tok2\nExpiry=9999999999\n"))
        .mount(&mock_server)
        .await;

    // First HashCheck gets 401 with tok1, invalidating TokenManager's cache; next call must use tok2!
    Mock::given(method("POST"))
        .and(path("/5084965799730810217"))
        .and(header("Authorization", "Bearer tok1"))
        .respond_with(ResponseTemplate::new(401).set_body_string("token expired"))
        .mount(&mock_server)
        .await;
    Mock::given(method("POST"))
        .and(path("/5084965799730810217"))
        .and(header("Authorization", "Bearer tok2"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_empty_remote_matches_bytes()))
        .mount(&mock_server)
        .await;

    let api = lumen_core::api::Api::new(
        lumen_core::api::ApiOptions {
            custom_endpoint_base: Some(base_url.clone()),
            ..Default::default()
        },
        "androidId=12345&Email=user@example.com&Token=oauth2_token",
    )
    .unwrap();
    let err = api.find_remote_media_by_hash(b"01234567890123456789").await.unwrap_err();
    assert!(err.to_string().contains("401"));
    // Next call must bypass the invalidated tok1 and exchange for tok2
    assert!(api.find_remote_media_by_hash(b"01234567890123456789").await.unwrap().is_none());

    // Now test full UploadEngine retry on transient 503 during get_upload_token
    Mock::given(method("POST"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .respond_with(ResponseTemplate::new(503).set_body_string("Service Unavailable"))
        .up_to_n_times(1)
        .mount(&mock_server)
        .await;
    Mock::given(method("POST"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .respond_with(ResponseTemplate::new(200).insert_header("X-GUploader-UploadID", "sess_retry"))
        .mount(&mock_server)
        .await;

    Mock::given(method("PUT"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_commit_token_bytes()))
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/16538846908252377752"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_commit_response_bytes("retry_media_key")))
        .mount(&mock_server)
        .await;

    let engine = UploadEngine::new(cfg_mgr, Some(hash_cache), album_registry);
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(100);
    let drain_handle = tokio::spawn(async move {
        while let Some(_event) = event_rx.recv().await {}
    });

    let opts = UploadOptions {
        threads: 1,
        delete_from_host: true,
        custom_endpoint_base: Some(base_url),
        ..Default::default()
    };

    let results = engine
        .run_upload(
            vec![UploadWorkItem::Single {
                path: photo.clone(),
                account_email: "user@example.com".to_string(),
            }],
            opts,
            event_tx,
        )
        .await
        .unwrap();
    let _ = drain_handle.await;

    assert_eq!(results.len(), 1);
    assert!(!results[0].is_error, "expected retry to succeed: {}", results[0].error_message);
    assert_eq!(results[0].media_key, "retry_media_key");
    assert!(!photo.exists(), "local file should be deleted after successful batch");
}

/// Scenario 8: Resuming a session where server already received 100% of bytes finalizes cleanly.
#[tokio::test]
async fn test_scenario_8_resume_when_server_received_all_bytes_finalizes() {
    let mock_server = MockServer::start().await;
    let base_url = mock_server.uri();

    let dir = tempdir().unwrap();
    let payload = b"already uploaded payload bytes 123456";
    let photo = create_test_file(dir.path(), "resumed.jpg", payload);
    let (_raw, sha1_b64) = lumen_core::dedup::calculate_file_sha1(&photo).unwrap();

    let cfg_mgr = Arc::new(ConfigManager::new_with_mock_keyring(dir.path().join("lumen.config")));
    cfg_mgr
        .save_credential("user@example.com", "androidId=123&Email=user@example.com&Token=tok")
        .unwrap();

    let hash_cache = HashCache::open(&dir.path().join("hashes.db")).unwrap();
    hash_cache
        .save_upload_session("user@example.com", &sha1_b64, payload.len() as i64, "resume_tok_full")
        .unwrap();
    let album_registry = AlbumRegistry::new(dir.path().join("albums.json")).unwrap();

    Mock::given(method("POST"))
        .and(path("/auth"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Auth=bearer_ok\nExpiry=3600\n"))
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/5084965799730810217"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_empty_remote_matches_bytes()))
        .mount(&mock_server)
        .await;

    // Query reports 100% of bytes already received
    Mock::given(method("POST"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .and(header("X-Goog-Upload-Command", "query"))
        .and(query_param("upload_id", "resume_tok_full"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-goog-upload-size-received", payload.len().to_string()),
        )
        .mount(&mock_server)
        .await;

    // Finalize at offset == file_size
    Mock::given(method("PUT"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .and(query_param("upload_id", "resume_tok_full"))
        .and(header("X-Goog-Upload-Offset", payload.len().to_string()))
        .and(headers("X-Goog-Upload-Command", vec!["upload", "finalize"]))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_commit_token_bytes()))
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/16538846908252377752"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_commit_response_bytes("resumed_full_key")))
        .mount(&mock_server)
        .await;

    let engine = UploadEngine::new(cfg_mgr, Some(hash_cache), album_registry);
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(100);
    let drain = tokio::spawn(async move { while event_rx.recv().await.is_some() {} });

    let results = engine
        .run_upload(
            vec![UploadWorkItem::Single {
                path: photo,
                account_email: "user@example.com".to_string(),
            }],
            UploadOptions {
                threads: 1,
                custom_endpoint_base: Some(base_url),
                ..Default::default()
            },
            event_tx,
        )
        .await
        .unwrap();
    let _ = drain.await;

    assert_eq!(results.len(), 1);
    assert!(!results[0].is_error, "expected finalize to succeed: {}", results[0].error_message);
    assert_eq!(results[0].media_key, "resumed_full_key");
}

/// Scenario 9: Dry run with album_name does not call create/add album endpoints.
#[tokio::test]
async fn test_scenario_9_dry_run_skips_album_attachment() {
    let mock_server = MockServer::start().await;
    let base_url = mock_server.uri();

    let dir = tempdir().unwrap();
    let photo = create_test_file(dir.path(), "dryrun.jpg", b"dry run image bytes");

    let cfg_mgr = Arc::new(ConfigManager::new_with_mock_keyring(dir.path().join("lumen.config")));
    cfg_mgr
        .save_credential("user@example.com", "androidId=123&Email=user@example.com&Token=tok")
        .unwrap();

    let hash_cache = HashCache::open(&dir.path().join("hashes.db")).unwrap();
    let album_registry = AlbumRegistry::new(dir.path().join("albums.json")).unwrap();

    Mock::given(method("POST"))
        .and(path("/auth"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Auth=bearer_ok\nExpiry=3600\n"))
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/5084965799730810217"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_empty_remote_matches_bytes()))
        .mount(&mock_server)
        .await;

    // Album endpoints must never be hit on dry run
    Mock::given(method("POST"))
        .and(path("/8386163679468898444"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&mock_server)
        .await;

    let engine = UploadEngine::new(cfg_mgr, Some(hash_cache), album_registry);
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(100);
    let drain = tokio::spawn(async move {
        let mut events = Vec::new();
        while let Some(ev) = event_rx.recv().await {
            events.push(ev);
        }
        events
    });

    let results = engine
        .run_upload(
            vec![UploadWorkItem::Single {
                path: photo,
                account_email: "user@example.com".to_string(),
            }],
            UploadOptions {
                threads: 1,
                dry_run: true,
                album_name: "ShouldNotCreate".to_string(),
                custom_endpoint_base: Some(base_url),
                ..Default::default()
            },
            event_tx,
        )
        .await
        .unwrap();
    let events = drain.await.unwrap();

    assert_eq!(results.len(), 1);
    assert!(!results[0].is_error);
    assert_eq!(results[0].skip_code, "dry-run");
    assert!(events.iter().any(|e| matches!(
        e,
        UploadEvent::BatchComplete {
            uploaded: 1,
            skipped: 0,
            failed: 0
        }
    )));
}

/// Scenario 10: Album creation failure does not corrupt counts when a skipped duplicate shares the same media_key.
#[tokio::test]
async fn test_scenario_10_album_failure_ignores_skipped_duplicate_same_media_key() {
    let mock_server = MockServer::start().await;
    let base_url = mock_server.uri();

    let dir = tempdir().unwrap();
    let cached_dup = create_test_file(dir.path(), "cached_dup.jpg", b"duplicate image bytes");
    let fresh_photo = create_test_file(dir.path(), "fresh.jpg", b"fresh image bytes");
    let (_raw, dup_sha1) = lumen_core::dedup::calculate_file_sha1(&cached_dup).unwrap();

    let cfg_mgr = Arc::new(ConfigManager::new_with_mock_keyring(dir.path().join("lumen.config")));
    cfg_mgr
        .save_credential("user@example.com", "androidId=123&Email=user@example.com&Token=tok")
        .unwrap();

    let hash_cache = HashCache::open(&dir.path().join("hashes.db")).unwrap();
    hash_cache
        .put(
            "user@example.com",
            &dup_sha1,
            "shared_media_key",
            b"duplicate image bytes".len() as i64,
            1000,
        )
        .unwrap();
    let album_registry = AlbumRegistry::new(dir.path().join("albums.json")).unwrap();

    Mock::given(method("POST"))
        .and(path("/auth"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Auth=bearer_ok\nExpiry=3600\n"))
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/5084965799730810217"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_empty_remote_matches_bytes()))
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .respond_with(ResponseTemplate::new(200).insert_header("X-GUploader-UploadID", "tok_album_fail"))
        .mount(&mock_server)
        .await;

    Mock::given(method("PUT"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_commit_token_bytes()))
        .mount(&mock_server)
        .await;

    // Commit returns the exact same media_key as the locally cached duplicate
    Mock::given(method("POST"))
        .and(path("/16538846908252377752"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_commit_response_bytes("shared_media_key")))
        .mount(&mock_server)
        .await;

    // Album creation fails with 500
    Mock::given(method("POST"))
        .and(path("/8386163679468898444"))
        .respond_with(ResponseTemplate::new(500).set_body_string("album backend error"))
        .mount(&mock_server)
        .await;

    let engine = UploadEngine::new(cfg_mgr, Some(hash_cache), album_registry);
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(100);
    let drain = tokio::spawn(async move {
        let mut events = Vec::new();
        while let Some(ev) = event_rx.recv().await {
            events.push(ev);
        }
        events
    });

    let _results = engine
        .run_upload(
            vec![
                UploadWorkItem::Single {
                    path: cached_dup,
                    account_email: "user@example.com".to_string(),
                },
                UploadWorkItem::Single {
                    path: fresh_photo,
                    account_email: "user@example.com".to_string(),
                },
            ],
            UploadOptions {
                threads: 1,
                album_name: "BrokenAlbum".to_string(),
                custom_endpoint_base: Some(base_url),
                ..Default::default()
            },
            event_tx,
        )
        .await
        .unwrap();
    let events = drain.await.unwrap();

    assert!(
        events.iter().any(|e| matches!(
            e,
            UploadEvent::BatchComplete {
                uploaded: 0,
                skipped: 1,
                failed: 1
            }
        )),
        "expected uploaded=0, skipped=1, failed=1 in events: {:?}",
        events
    );
}

/// Scenario 11: Transient commit failure retries commit without re-uploading file bytes.
#[tokio::test]
async fn test_scenario_11_commit_retry_reuses_finalize_token() {
    let mock_server = MockServer::start().await;
    let base_url = mock_server.uri();

    let dir = tempdir().unwrap();
    let photo = create_test_file(dir.path(), "commit_retry.jpg", b"payload bytes");

    let cfg_mgr = Arc::new(ConfigManager::new_with_mock_keyring(dir.path().join("lumen.config")));
    cfg_mgr
        .save_credential("user@example.com", "androidId=123&Email=user@example.com&Token=tok")
        .unwrap();

    let hash_cache = HashCache::open(&dir.path().join("hashes.db")).unwrap();
    let album_registry = AlbumRegistry::new(dir.path().join("albums.json")).unwrap();

    Mock::given(method("POST"))
        .and(path("/auth"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Auth=bearer_ok\nExpiry=3600\n"))
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/5084965799730810217"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_empty_remote_matches_bytes()))
        .mount(&mock_server)
        .await;

    // Upload token and PUT upload must only happen once
    Mock::given(method("POST"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .respond_with(ResponseTemplate::new(200).insert_header("X-GUploader-UploadID", "tok_once"))
        .expect(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("PUT"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_commit_token_bytes()))
        .expect(1)
        .mount(&mock_server)
        .await;

    // First commit fails with 503, second succeeds
    Mock::given(method("POST"))
        .and(path("/16538846908252377752"))
        .respond_with(ResponseTemplate::new(503).set_body_string("try again"))
        .up_to_n_times(1)
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/16538846908252377752"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_commit_response_bytes("committed_after_retry")))
        .expect(1)
        .mount(&mock_server)
        .await;

    let engine = UploadEngine::new(cfg_mgr, Some(hash_cache), album_registry);
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(100);
    let drain = tokio::spawn(async move { while event_rx.recv().await.is_some() {} });

    let results = engine
        .run_upload(
            vec![UploadWorkItem::Single {
                path: photo,
                account_email: "user@example.com".to_string(),
            }],
            UploadOptions {
                threads: 1,
                custom_endpoint_base: Some(base_url),
                ..Default::default()
            },
            event_tx,
        )
        .await
        .unwrap();
    let _ = drain.await;

    assert_eq!(results.len(), 1);
    assert!(!results[0].is_error, "expected commit retry to succeed: {}", results[0].error_message);
    assert_eq!(results[0].media_key, "committed_after_retry");
}

/// Scenario 12: Live Photo skipped because only photo exists remotely must NOT delete local files when delete_from_host=true.
#[tokio::test]
async fn test_scenario_12_live_photo_partial_remote_skip_does_not_delete_local_files() {
    let mock_server = MockServer::start().await;
    let base_url = mock_server.uri();

    let dir = tempdir().unwrap();
    let photo = create_test_file(dir.path(), "IMG_1001.jpg", b"still image bytes");
    let video = create_test_file(dir.path(), "IMG_1001.mov", b"motion video bytes");
    let (photo_sha1_raw, _) = lumen_core::dedup::calculate_file_sha1(&photo).unwrap();

    let cfg_mgr = Arc::new(ConfigManager::new_with_mock_keyring(dir.path().join("lumen.config")));
    cfg_mgr
        .save_credential("user@example.com", "androidId=123&Email=user@example.com&Token=tok")
        .unwrap();

    let hash_cache = HashCache::open(&dir.path().join("hashes.db")).unwrap();
    let album_registry = AlbumRegistry::new(dir.path().join("albums.json")).unwrap();

    Mock::given(method("POST"))
        .and(path("/auth"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Auth=bearer_ok\nExpiry=3600\n"))
        .mount(&mock_server)
        .await;

    // Return remote match ONLY when the photo SHA1 is queried; video query returns empty match.
    Mock::given(method("POST"))
        .and(path("/5084965799730810217"))
        .and(wiremock::matchers::body_bytes(
            {
                let req = lumen_core::proto::lumen::HashCheck {
                    field1: Some(lumen_core::proto::lumen::hash_check::Field1Type {
                        field1: Some(lumen_core::proto::lumen::hash_check::field1_type::Field1Type {
                            sha1_hash: photo_sha1_raw,
                        }),
                        field2: Some(lumen_core::proto::lumen::hash_check::field1_type::Field2Type {}),
                    }),
                };
                let mut buf = Vec::new();
                prost::Message::encode(&req, &mut buf).unwrap();
                buf
            },
        ))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_matched_remote_matches_bytes("existing_photo_key")))
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/5084965799730810217"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_empty_remote_matches_bytes()))
        .mount(&mock_server)
        .await;

    let items = vec![UploadWorkItem::LivePhoto {
        pair: lumen_core::livephoto::LivePhotoPair {
            content_identifier: "CID_1001".to_string(),
            photo_path: photo.clone(),
            video_path: video.clone(),
        },
        account_email: "user@example.com".to_string(),
    }];

    let engine = UploadEngine::new(cfg_mgr, Some(hash_cache), album_registry);
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(100);
    let drain = tokio::spawn(async move { while event_rx.recv().await.is_some() {} });

    let results = engine
        .run_upload(
            items,
            UploadOptions {
                threads: 1,
                pair_live_photos: true,
                update_existing_photos_to_live: false,
                delete_from_host: true,
                custom_endpoint_base: Some(base_url),
                ..Default::default()
            },
            event_tx,
        )
        .await
        .unwrap();
    let _ = drain.await;

    assert_eq!(results.len(), 1);
    assert!(results[0].skipped);
    assert_eq!(results[0].skip_code, "live-photo-partial-remote");
    assert!(photo.exists(), "photo must remain when partial Live Photo was skipped");
    assert!(video.exists(), "un-uploaded video must NOT be deleted by delete_from_host");
}

/// Scenario 13: TokenManager singleflight & match-only invalidation + force_upload preserves retry session.
#[tokio::test]
async fn test_scenario_13_auth_singleflight_and_force_upload_retry_session() {
    use lumen_core::auth::TokenManager;
    use lumen_core::api::{Api, ApiOptions};

    let mock_server = MockServer::start().await;
    let base_url = mock_server.uri();

    // Concurrent bearer_token calls with a shared TokenManager should only hit /auth once
    Mock::given(method("POST"))
        .and(path("/auth"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("Auth=fresh_tok_1\nExpiry=3600\n")
                .set_delay(std::time::Duration::from_millis(40)),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let tm = TokenManager::new("user@example.com");
    let mut api = Api::new(
        ApiOptions {
            account: "user@example.com".to_string(),
            custom_endpoint_base: Some(base_url.clone()),
            ..Default::default()
        },
        "androidId=123&Email=user@example.com&Token=tok",
    )
    .unwrap();
    api.set_token_manager(tm.clone());
    let api = Arc::new(api);

    let (r1, r2, r3) = tokio::join!(api.bearer_token(), api.bearer_token(), api.bearer_token());
    assert_eq!(r1.unwrap(), "fresh_tok_1");
    assert_eq!(r2.unwrap(), "fresh_tok_1");
    assert_eq!(r3.unwrap(), "fresh_tok_1");

    // Stale token invalidation must NOT wipe a newer token
    tm.set_token("fresh_tok_2".to_string(), std::time::Duration::from_secs(3600));
    tm.invalidate_if_matches("fresh_tok_1");
    assert_eq!(tm.get_cached_token().as_deref(), Some("fresh_tok_2"));
    tm.invalidate_if_matches("fresh_tok_2");
    assert_eq!(tm.get_cached_token(), None);

    // force_upload=true must clear any prior session initially, then reuse its new session across retries
    let retry_server = MockServer::start().await;
    let retry_url = retry_server.uri();
    let dir = tempdir().unwrap();
    let photo = create_test_file(dir.path(), "force_retry.jpg", b"force upload retry payload");

    let cfg_mgr = Arc::new(ConfigManager::new_with_mock_keyring(dir.path().join("lumen.config")));
    cfg_mgr
        .save_credential("user@example.com", "androidId=123&Email=user@example.com&Token=tok")
        .unwrap();

    let hash_cache = HashCache::open(&dir.path().join("hashes.db")).unwrap();
    let album_registry = AlbumRegistry::new(dir.path().join("albums.json")).unwrap();

    Mock::given(method("POST"))
        .and(path("/auth"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Auth=bearer_ok\nExpiry=3600\n"))
        .mount(&retry_server)
        .await;

    // get_upload_token should only be called ONCE even though first PUT fails 503
    Mock::given(method("POST"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .and(wiremock::matchers::header("X-Upload-Content-Length", "26"))
        .respond_with(ResponseTemplate::new(200).insert_header("X-GUploader-UploadID", "up_force_1"))
        .expect(1)
        .mount(&retry_server)
        .await;

    // query_upload_status on retry returns 0 so second PUT completes cleanly
    Mock::given(method("POST"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .and(wiremock::matchers::header("X-Goog-Upload-Command", "query"))
        .respond_with(ResponseTemplate::new(200).insert_header("x-goog-upload-size-received", "0"))
        .expect(1)
        .mount(&retry_server)
        .await;

    Mock::given(method("PUT"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .respond_with(ResponseTemplate::new(503).set_body_string("temporary error"))
        .up_to_n_times(1)
        .mount(&retry_server)
        .await;

    Mock::given(method("PUT"))
        .and(path("/data/upload/uploadmedia/interactive"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_commit_token_bytes()))
        .mount(&retry_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/16538846908252377752"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(helper_commit_response_bytes("force_retry_key")))
        .mount(&retry_server)
        .await;

    let engine = UploadEngine::new(cfg_mgr, Some(hash_cache), album_registry);
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(100);
    let drain = tokio::spawn(async move { while event_rx.recv().await.is_some() {} });

    let results = engine
        .run_upload(
            vec![UploadWorkItem::Single {
                path: photo,
                account_email: "user@example.com".to_string(),
            }],
            UploadOptions {
                threads: 1,
                force_upload: true,
                custom_endpoint_base: Some(retry_url),
                ..Default::default()
            },
            event_tx,
        )
        .await
        .unwrap();
    let _ = drain.await;

    assert_eq!(results.len(), 1);
    assert!(!results[0].is_error, "expected retry upload to succeed: {}", results[0].error_message);
    assert_eq!(results[0].media_key, "force_retry_key");
}
