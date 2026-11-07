use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use std::sync::{Arc, Mutex};
use rusqlite::{params, Connection};
use sha1::{Digest, Sha1};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DedupError {
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
}

pub fn calculate_file_sha1(path: &Path) -> io::Result<(Vec<u8>, String)> {
    let mut file = File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let hash_bytes = hasher.finalize().to_vec();
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&hash_bytes);
    Ok((hash_bytes, b64))
}

#[derive(Clone)]
pub struct HashCache {
    conn: Arc<Mutex<Connection>>,
}

impl HashCache {
    pub fn open(db_path: &Path) -> Result<Self, DedupError> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
            }
        }
        let conn = Connection::open(db_path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(db_path, std::fs::Permissions::from_mode(0o600));
        }
        let cache = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        cache.init_schema()?;
        Ok(cache)
    }

    pub fn in_memory() -> Result<Self, DedupError> {
        let conn = Connection::open_in_memory()?;
        let cache = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        cache.init_schema()?;
        Ok(cache)
    }

    fn init_schema(&self) -> Result<(), DedupError> {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;");

        // Check for table migration
        let has_acc_col: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('file_hashes') WHERE name = 'account_email'",
            [],
            |r| r.get(0),
        ).unwrap_or(0);

        if has_acc_col == 0 {
            let table_exists: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='file_hashes'",
                [],
                |r| r.get(0),
            ).unwrap_or(0);
            if table_exists > 0 {
                let _ = conn.execute("DROP TABLE IF EXISTS file_hashes", []);
            }
        }

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS file_hashes (
                account_email TEXT NOT NULL DEFAULT '',
                sha1_b64 TEXT NOT NULL,
                media_key TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                mtime INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                PRIMARY KEY (account_email, sha1_b64, file_size)
            );
            CREATE INDEX IF NOT EXISTS idx_file_hashes_acc ON file_hashes(account_email);",
        )?;

        Ok(())
    }

    pub fn get(&self, account_email: &str, sha1_b64: &str, file_size: i64) -> Result<Option<String>, DedupError> {
        if account_email.is_empty() || sha1_b64.is_empty() {
            return Ok(None);
        }
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT media_key FROM file_hashes WHERE account_email = ?1 AND sha1_b64 = ?2 AND file_size = ?3"
        )?;
        let mut rows = stmt.query(params![account_email, sha1_b64, file_size])?;
        if let Some(row) = rows.next()? {
            let key: String = row.get(0)?;
            Ok(Some(key))
        } else {
            Ok(None)
        }
    }

    pub fn put(&self, account_email: &str, sha1_b64: &str, media_key: &str, file_size: i64, mtime: i64) -> Result<(), DedupError> {
        if account_email.is_empty() || sha1_b64.is_empty() || media_key.is_empty() {
            return Ok(());
        }
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        conn.execute(
            "INSERT INTO file_hashes (account_email, sha1_b64, media_key, file_size, mtime, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(account_email, sha1_b64, file_size) DO UPDATE SET
                media_key = excluded.media_key,
                mtime = excluded.mtime,
                created_at = excluded.created_at;",
            params![account_email, sha1_b64, media_key, file_size, mtime, now],
        )?;
        Ok(())
    }

    pub fn count(&self, account_email: Option<&str>) -> Result<i64, DedupError> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = match account_email {
            Some(email) if !email.is_empty() => {
                conn.query_row(
                    "SELECT COUNT(*) FROM file_hashes WHERE account_email = ?1",
                    params![email],
                    |r| r.get(0),
                )?
            }
            _ => {
                conn.query_row("SELECT COUNT(*) FROM file_hashes", [], |r| r.get(0))?
            }
        };
        Ok(count)
    }

    pub fn clear(&self, account_email: Option<&str>) -> Result<usize, DedupError> {
        let conn = self.conn.lock().unwrap();
        let rows = match account_email {
            Some(email) if !email.is_empty() => {
                conn.execute("DELETE FROM file_hashes WHERE account_email = ?1", params![email])?
            }
            _ => conn.execute("DELETE FROM file_hashes", [])?,
        };
        let _ = conn.execute("VACUUM", []);
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_cache_crud() {
        let cache = HashCache::in_memory().unwrap();
        let email1 = "alice@example.com";
        let email2 = "bob@example.com";
        let hash = "da39a3ee5e6b4b0d3255bfef95601890afd80709";
        let size = 1024;

        assert_eq!(cache.get(email1, hash, size).unwrap(), None);

        cache.put(email1, hash, "media_key_1", size, 12345).unwrap();
        assert_eq!(cache.get(email1, hash, size).unwrap(), Some("media_key_1".to_string()));

        // Different account should not see it (per-account isolation!)
        assert_eq!(cache.get(email2, hash, size).unwrap(), None);

        // Put for email2
        cache.put(email2, hash, "media_key_2", size, 12345).unwrap();
        assert_eq!(cache.get(email2, hash, size).unwrap(), Some("media_key_2".to_string()));

        assert_eq!(cache.count(Some(email1)).unwrap(), 1);
        assert_eq!(cache.count(None).unwrap(), 2);

        // Clear email1
        cache.clear(Some(email1)).unwrap();
        assert_eq!(cache.get(email1, hash, size).unwrap(), None);
        assert_eq!(cache.get(email2, hash, size).unwrap(), Some("media_key_2".to_string()));
    }
}
