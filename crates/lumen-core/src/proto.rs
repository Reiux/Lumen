//! Protobuf message types generated from .proto definitions

#[allow(clippy::all)]
pub mod lumen {
    include!(concat!(env!("OUT_DIR"), "/lumen.rs"));
}

#[cfg(test)]
mod tests {
    use super::lumen::*;
    use prost::Message;

    #[test]
    fn test_add_media_to_album_serialization_matches_go() {
        let msg = AddMediaToAlbum {
            media_keys: vec!["key1".to_string(), "key2".to_string()],
            album_media_key: "albumKey123".to_string(),
            field5: Some(add_media_to_album::Field5Type { field1: 2 }),
            device_info: Some(add_media_to_album::Field6Type {
                model: "Pixel 7".to_string(),
                make: "Google".to_string(),
                android_api_version: 33,
            }),
            timestamp: 1700000000,
        };

        let mut buf = Vec::new();
        msg.encode(&mut buf).unwrap();
        let expected_hex = "0a046b6579310a046b657932120b616c62756d4b65793132332a02080232131a07506978656c20372206476f6f676c6528213880e2cfaa06";
        let actual_hex: String = buf.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(actual_hex, expected_hex);
    }

    #[test]
    fn test_commit_token_serialization_matches_go() {
        let token = CommitToken {
            field1: 2,
            field2: b"test-opaque-scotty-data".to_vec(),
        };

        let mut buf = Vec::new();
        token.encode(&mut buf).unwrap();

        let expected_hex = "08021217746573742d6f70617175652d73636f7474792d64617461";
        let actual_hex: String = buf.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(actual_hex, expected_hex);

        let decoded = CommitToken::decode(&buf[..]).unwrap();
        assert_eq!(decoded.field1, 2);
        assert_eq!(decoded.field2, b"test-opaque-scotty-data");
    }

    #[test]
    fn test_commit_upload_serialization_matches_go() {
        let msg = CommitUpload {
            field1: Some(commit_upload::Field1Type {
                field1: Some(commit_upload::field1_type::Field1Type {
                    field1: 2,
                    field2: b"scotty-token".to_vec(),
                }),
                file_name: "photo.jpg".to_string(),
                sha1_hash: b"01234567890123456789".to_vec(),
                field4: Some(commit_upload::field1_type::Field4Type {
                    file_last_modified_timestamp: 1690000000,
                    field2: 0,
                }),
                quality: 1,
                ..Default::default()
            }),
            field2: Some(commit_upload::Field2Type {
                model: "Pixel 7".to_string(),
                make: "Google".to_string(),
                android_api_version: 33,
            }),
            field3: Vec::new(),
        };

        let mut buf = Vec::new();
        msg.encode(&mut buf).unwrap();
        let expected_hex = "0a3d0a100802120c73636f7474792d746f6b656e120970686f746f2e6a70671a14303132333435363738393031323334353637383922060880b5eda506380112131a07506978656c20372206476f6f676c652821";
        let actual_hex: String = buf.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(actual_hex, expected_hex);
    }

    #[test]
    fn test_commit_upload_response_serialization_matches_go() {
        let msg = CommitUploadResponse {
            field1: Some(commit_upload_response::Field1Type {
                field1: Some(commit_upload_response::field1_type::Field1Type {
                    field1: 2,
                    field2: b"resp-token".to_vec(),
                }),
                field2: 1,
                field3: Some(commit_upload_response::field1_type::Field3Type {
                    media_key: "AF1Qip_test_media_key".to_string(),
                    ..Default::default()
                }),
            }),
            field2: None,
        };

        let mut buf = Vec::new();
        msg.encode(&mut buf).unwrap();
        let expected_hex = "0a2b0a0e0802120a726573702d746f6b656e10011a170a154146315169705f746573745f6d656469615f6b6579";
        let actual_hex: String = buf.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(actual_hex, expected_hex);
    }

    #[test]
    fn test_create_album_serialization_matches_go() {
        let msg = CreateAlbum {
            album_name: "Vacation".to_string(),
            timestamp: 1699999999,
            field3: 1,
            media_keys: vec![create_album::Field4Type {
                field1: Some(create_album::field4_type::Field1Type {
                    media_key: "media_key_1".to_string(),
                }),
            }],
            field6: Some(create_album::Field6Type {}),
            field7: Some(create_album::Field7Type { field1: 3 }),
            device_info: Some(create_album::Field8Type {
                model: "Pixel 7".to_string(),
                make: "Google".to_string(),
                android_api_version: 33,
            }),
        };

        let mut buf = Vec::new();
        msg.encode(&mut buf).unwrap();
        let expected_hex = "0a085661636174696f6e10ffe1cfaa061801220f0a0d0a0b6d656469615f6b65795f3132003a02080342131a07506978656c20372206476f6f676c652821";
        let actual_hex: String = buf.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(actual_hex, expected_hex);
    }

    #[test]
    fn test_create_album_response_serialization_matches_go() {
        let msg = CreateAlbumResponse {
            field1: Some(create_album_response::Field1Type {
                album_media_key: "AF1Qip_album_key".to_string(),
            }),
            field2: vec!["id1".to_string(), "id2".to_string()],
            field3: Some(create_album_response::Field3Type {
                field1: vec![create_album_response::field3_type::Field1Type {
                    input_media_key: "input_key".to_string(),
                    result_key: "result_key".to_string(),
                    status: 1,
                }],
            }),
        };

        let mut buf = Vec::new();
        msg.encode(&mut buf).unwrap();
        let expected_hex = "0a120a104146315169705f616c62756d5f6b6579120369643112036964321a1b0a190a09696e7075745f6b6579120a726573756c745f6b65791801";
        let actual_hex: String = buf.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(actual_hex, expected_hex);
    }

    #[test]
    fn test_create_media_items_request_serialization_matches_go() {
        let msg = CreateMediaItemsRequest {
            blueprint_array: vec![MediaItemBlueprint {
                upload_token: b"upload-token-abc".to_vec(),
                file_name: "live.heic".to_string(),
                source_sha1: b"source-sha1-bytes-20".to_vec(),
                storage_policy: 1,
                upload_quality: 2,
                ..Default::default()
            }],
            upload_device_info: Some(UploadDeviceInfo {
                model: "Pixel 7".to_string(),
                make: "Google".to_string(),
                android_api_version: 33,
            }),
            result_item_mask: vec![0x01, 0x02, 0x03],
        };

        let mut buf = Vec::new();
        msg.encode(&mut buf).unwrap();
        let expected_hex = "0a370a1075706c6f61642d746f6b656e2d61626312096c6976652e686569631a14736f757263652d736861312d62797465732d32303801500212131a07506978656c20372206476f6f676c6528212a03010203";
        let actual_hex: String = buf.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(actual_hex, expected_hex);
    }

    #[test]
    fn test_get_upload_token_serialization_matches_go() {
        let req = GetUploadToken {
            f1: 2,
            f2: 2,
            f3: 1,
            f4: 3,
            file_size_bytes: 1048576,
        };

        let mut buf = Vec::new();
        req.encode(&mut buf).unwrap();

        let expected_hex = "080210021801200338808040";
        let actual_hex: String = buf.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(actual_hex, expected_hex);
    }

    #[test]
    fn test_hash_check_serialization_matches_go() {
        let msg = HashCheck {
            field1: Some(hash_check::Field1Type {
                field1: Some(hash_check::field1_type::Field1Type {
                    sha1_hash: b"12345678901234567890".to_vec(),
                }),
                field2: Some(hash_check::field1_type::Field2Type {}),
            }),
        };

        let mut buf = Vec::new();
        msg.encode(&mut buf).unwrap();
        let expected_hex = "0a1a0a160a1431323334353637383930313233343536373839301200";
        let actual_hex: String = buf.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(actual_hex, expected_hex);
    }

    #[test]
    fn test_remote_matches_serialization_matches_go() {
        let msg = RemoteMatches {
            field1: Some(remote_matches::Field1Type {
                field2: Some(remote_matches::field1_type::Field2Type {
                    field1: Some(remote_matches::field1_type::field2_type::Field1Type {
                        sha1_hash: b"hash_sample_20bytes!".to_vec(),
                    }),
                    field2: Some(remote_matches::field1_type::field2_type::Field2Type {
                        media_key: "AF1Qip_existing_key".to_string(),
                        field6: None,
                    }),
                }),
            }),
        };

        let mut buf = Vec::new();
        msg.encode(&mut buf).unwrap();
        let expected_hex = "0a31122f0a160a14686173685f73616d706c655f323062797465732112150a134146315169705f6578697374696e675f6b6579";
        let actual_hex: String = buf.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(actual_hex, expected_hex);
    }
}
