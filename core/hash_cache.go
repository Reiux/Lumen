package core

import (
	"database/sql"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"

	_ "modernc.org/sqlite"
)

// HashCache provides local persistent caching of uploaded file hashes and their
// corresponding Google Photos media keys, keyed by (account_email, sha1_b64, file_size).
// This avoids redundant remote checks and re-uploads when files are processed across multiple sessions.
type HashCache struct {
	dbPath string
	db     *sql.DB
	mu     sync.RWMutex
}

var (
	globalHashCache   *HashCache
	globalHashCacheMu sync.Mutex
)

// InitGlobalHashCache initializes the singleton hash cache at the specified path.
func InitGlobalHashCache(dbPath string) (*HashCache, error) {
	globalHashCacheMu.Lock()
	defer globalHashCacheMu.Unlock()

	if globalHashCache != nil {
		if globalHashCache.dbPath == dbPath {
			return globalHashCache, nil
		}
		_ = globalHashCache.Close()
	}

	cache, err := NewHashCache(dbPath)
	if err != nil {
		return nil, err
	}
	globalHashCache = cache
	return cache, nil
}

// GetGlobalHashCache returns the active singleton hash cache, or nil if uninitialized.
func GetGlobalHashCache() *HashCache {
	globalHashCacheMu.Lock()
	defer globalHashCacheMu.Unlock()
	return globalHashCache
}

// NewHashCache opens or creates a SQLite-backed hash cache database.
func NewHashCache(dbPath string) (*HashCache, error) {
	if err := os.MkdirAll(filepath.Dir(dbPath), 0o700); err != nil {
		return nil, fmt.Errorf("failed to create directory for hash cache: %w", err)
	}

	db, err := sql.Open("sqlite", dbPath)
	if err != nil {
		return nil, fmt.Errorf("failed to open hash cache database: %w", err)
	}

	// Enable WAL mode and normal synchronous for performant concurrency
	_, _ = db.Exec("PRAGMA journal_mode=WAL;")
	_, _ = db.Exec("PRAGMA synchronous=NORMAL;")

	cache := &HashCache{
		dbPath: dbPath,
		db:     db,
	}

	if err := cache.initSchema(); err != nil {
		_ = db.Close()
		return nil, fmt.Errorf("failed to initialize hash cache schema: %w", err)
	}

	return cache, nil
}

func (c *HashCache) initSchema() error {
	// Check if existing table lacks account_email column (from older version)
	var colCount int
	_ = c.db.QueryRow("SELECT COUNT(*) FROM pragma_table_info('file_hashes') WHERE name = 'account_email'").Scan(&colCount)
	if colCount == 0 {
		var tableExists int
		_ = c.db.QueryRow("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='file_hashes'").Scan(&tableExists)
		if tableExists > 0 {
			// Migrate by recreating with composite primary key
			_, _ = c.db.Exec("DROP TABLE IF EXISTS file_hashes;")
		}
	}

	schema := `
	CREATE TABLE IF NOT EXISTS file_hashes (
		account_email TEXT NOT NULL DEFAULT '',
		sha1_b64 TEXT NOT NULL,
		media_key TEXT NOT NULL,
		file_size INTEGER NOT NULL,
		mtime INTEGER NOT NULL,
		created_at INTEGER NOT NULL,
		PRIMARY KEY (account_email, sha1_b64, file_size)
	);
	CREATE INDEX IF NOT EXISTS idx_file_hashes_acc ON file_hashes(account_email);
	`
	_, err := c.db.Exec(schema)
	return err
}

// Get retrieves the media key for an account, SHA1 base64 hash, and file size.
func (c *HashCache) Get(accountEmail string, sha1_b64 string, fileSize int64) (string, bool, error) {
	if c == nil || c.db == nil || sha1_b64 == "" || accountEmail == "" {
		return "", false, nil
	}

	c.mu.RLock()
	defer c.mu.RUnlock()

	var mediaKey string
	err := c.db.QueryRow(
		"SELECT media_key FROM file_hashes WHERE account_email = ? AND sha1_b64 = ? AND file_size = ?",
		accountEmail, sha1_b64, fileSize,
	).Scan(&mediaKey)
	if err == sql.ErrNoRows {
		return "", false, nil
	}
	if err != nil {
		return "", false, err
	}
	return mediaKey, true, nil
}

// Put records or updates an (account, hash, size) -> mediaKey mapping.
func (c *HashCache) Put(accountEmail string, sha1_b64 string, mediaKey string, fileSize int64, mtime int64) error {
	if c == nil || c.db == nil || sha1_b64 == "" || mediaKey == "" || accountEmail == "" {
		return nil
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	now := time.Now().Unix()
	query := `
	INSERT INTO file_hashes (account_email, sha1_b64, media_key, file_size, mtime, created_at)
	VALUES (?, ?, ?, ?, ?, ?)
	ON CONFLICT(account_email, sha1_b64, file_size) DO UPDATE SET
		media_key = excluded.media_key,
		mtime = excluded.mtime,
		created_at = excluded.created_at;
	`
	_, err := c.db.Exec(query, accountEmail, sha1_b64, mediaKey, fileSize, mtime, now)
	return err
}

// Count returns the number of cached hashes for a specific account email (or all if email is empty).
func (c *HashCache) Count(accountEmail string) (int64, error) {
	if c == nil || c.db == nil {
		return 0, nil
	}

	c.mu.RLock()
	defer c.mu.RUnlock()

	var count int64
	var err error
	if accountEmail != "" {
		err = c.db.QueryRow("SELECT COUNT(*) FROM file_hashes WHERE account_email = ?", accountEmail).Scan(&count)
	} else {
		err = c.db.QueryRow("SELECT COUNT(*) FROM file_hashes").Scan(&count)
	}
	if err != nil {
		return 0, err
	}
	return count, nil
}

// Clear removes cached hashes for a specific account email (or all if email is empty).
func (c *HashCache) Clear(accountEmail string) (int64, error) {
	if c == nil || c.db == nil {
		return 0, nil
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	var res sql.Result
	var err error
	if accountEmail != "" {
		res, err = c.db.Exec("DELETE FROM file_hashes WHERE account_email = ?", accountEmail)
	} else {
		res, err = c.db.Exec("DELETE FROM file_hashes")
	}
	if err != nil {
		return 0, err
	}
	rowsAffected, _ := res.RowsAffected()
	_, _ = c.db.Exec("VACUUM")
	return rowsAffected, nil
}

// Close closes the underlying SQLite database.
func (c *HashCache) Close() error {
	if c == nil || c.db == nil {
		return nil
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	err := c.db.Close()
	c.db = nil
	return err
}
