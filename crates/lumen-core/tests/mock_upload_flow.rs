//! Integration tests using wiremock to test all 5 core upload scenarios.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::tempdir;
use prost::Message;
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path, header, query_param};

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
