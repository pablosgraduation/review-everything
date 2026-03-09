//! Persistent per-file review tracking via SQLite.

use std::collections::HashSet;
use std::path::Path;

use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::types::DisplayFile;

pub struct ReviewStore {
    conn: Connection,
}

impl ReviewStore {
    /// Opens (or creates) the review DB for the given git root.
    /// Returns `None` if the DB cannot be opened or initialized.
    pub fn open(git_root: &Path) -> Option<Self> {
        let db_path = db_path(git_root)?;

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok()?;
        }

        match Self::try_open(&db_path) {
            Some(store) => Some(store),
            None => {
                // Corrupt or schema mismatch — delete and retry once
                let _ = std::fs::remove_file(&db_path);
                Self::try_open(&db_path)
            }
        }
    }

    fn try_open(db_path: &Path) -> Option<Self> {
        let conn = Connection::open(db_path).ok()?;
        conn.pragma_update(None, "journal_mode", "WAL").ok()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS reviews (
                scope       TEXT NOT NULL,
                path        TEXT NOT NULL,
                hash        TEXT NOT NULL,
                reviewed_at TEXT NOT NULL,
                PRIMARY KEY (scope, path)
            );",
        )
        .ok()?;

        // Health check
        conn.execute_batch("SELECT 1 FROM reviews LIMIT 1").ok()?;

        Some(Self { conn })
    }

    /// Mark a file as reviewed. Silent on error.
    pub fn mark(&self, scope: &str, path: &str, hash: u64) {
        let hash_str = hash.to_string();
        let now = now_iso8601();
        let _ = self.conn.execute(
            "INSERT OR REPLACE INTO reviews (scope, path, hash, reviewed_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![scope, path, hash_str, now],
        );
    }

    /// Clear all reviews across all scopes. Silent on error.
    pub fn clear_all(&self) {
        let _ = self.conn.execute("DELETE FROM reviews", []);
    }

    /// Unmark a file. Silent on error.
    pub fn unmark(&self, scope: &str, path: &str) {
        let _ = self.conn.execute(
            "DELETE FROM reviews WHERE scope = ?1 AND path = ?2",
            rusqlite::params![scope, path],
        );
    }

    /// Returns the set of file indices that are positively reviewed (hash matches).
    /// Deletes stale entries (hash mismatch) as a side effect.
    /// Returns empty set on any error.
    pub fn reviewed_set(&self, scope: &str, files: &[DisplayFile]) -> HashSet<usize> {
        let mut set = HashSet::new();

        let mut stmt = match self.conn.prepare(
            "SELECT path, hash FROM reviews WHERE scope = ?1",
        ) {
            Ok(s) => s,
            Err(_) => return set,
        };

        let rows = match stmt.query_map(rusqlite::params![scope], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
            ))
        }) {
            Ok(r) => r,
            Err(_) => return set,
        };

        let mut db_entries: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for row in rows {
            if let Ok((path, hash)) = row {
                db_entries.insert(path, hash);
            }
        }

        for (idx, file) in files.iter().enumerate() {
            let path_str = file.path.to_string_lossy();
            if let Some(stored_hash) = db_entries.get(path_str.as_ref()) {
                let current_hash = file.content_hash.to_string();
                if *stored_hash == current_hash {
                    set.insert(idx);
                } else {
                    // Stale — clean up
                    let _ = self.conn.execute(
                        "DELETE FROM reviews WHERE scope = ?1 AND path = ?2 AND hash != ?3",
                        rusqlite::params![scope, path_str.as_ref(), current_hash],
                    );
                }
            }
        }

        set
    }
}

/// SHA256 hex of the absolute git root path, for use as DB filename.
fn repo_id(git_root: &Path) -> String {
    let canonical = git_root
        .canonicalize()
        .unwrap_or_else(|_| git_root.to_path_buf());
    let mut hasher = Sha256::new();
    hasher.update(canonical.to_string_lossy().as_bytes());
    format!("{:x}", hasher.finalize())
}

fn db_path(git_root: &Path) -> Option<std::path::PathBuf> {
    let config_dir = dirs_db()?;
    Some(config_dir.join(format!("{}.db", repo_id(git_root))))
}

fn dirs_db() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()?;
    Some(Path::new(&home).join(".config/re/reviews"))
}

fn now_iso8601() -> String {
    // Use a simple approach without chrono dependency
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Format as basic timestamp — not perfect ISO 8601 but functional
    format!("{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_display_file(path: &str, hash: u64) -> DisplayFile {
        DisplayFile {
            path: PathBuf::from(path),
            moved_from: None,
            language: "Rust".into(),
            status: crate::types::FileStatus::Modified,
            additions: 1,
            deletions: 1,
            rows: vec![],
            hunks: vec![],
            aligned_lines: vec![],
            content_hash: hash,
        }
    }

    #[test]
    fn mark_and_check() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = ReviewStore::open(tmp.path()).unwrap();
        let scope = "abc:def";

        let files = vec![
            make_display_file("src/main.rs", 12345),
            make_display_file("src/lib.rs", 67890),
        ];

        // Initially empty
        let set = store.reviewed_set(scope, &files);
        assert!(set.is_empty());

        // Mark first file
        store.mark(scope, "src/main.rs", 12345);
        let set = store.reviewed_set(scope, &files);
        assert!(set.contains(&0));
        assert!(!set.contains(&1));

        // Mark second file
        store.mark(scope, "src/lib.rs", 67890);
        let set = store.reviewed_set(scope, &files);
        assert!(set.contains(&0));
        assert!(set.contains(&1));
    }

    #[test]
    fn unmark() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = ReviewStore::open(tmp.path()).unwrap();
        let scope = "abc:def";

        let files = vec![make_display_file("src/main.rs", 12345)];

        store.mark(scope, "src/main.rs", 12345);
        let set = store.reviewed_set(scope, &files);
        assert!(set.contains(&0));

        store.unmark(scope, "src/main.rs");
        let set = store.reviewed_set(scope, &files);
        assert!(set.is_empty());
    }

    #[test]
    fn stale_hash_invalidation() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = ReviewStore::open(tmp.path()).unwrap();
        let scope = "abc:def";

        // Mark with old hash
        store.mark(scope, "src/main.rs", 12345);

        // Now file has different hash
        let files = vec![make_display_file("src/main.rs", 99999)];
        let set = store.reviewed_set(scope, &files);
        assert!(set.is_empty()); // hash mismatch → not reviewed
    }

    #[test]
    fn separate_scopes() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = ReviewStore::open(tmp.path()).unwrap();

        let files = vec![make_display_file("src/main.rs", 12345)];

        store.mark("scope_a", "src/main.rs", 12345);
        let set_a = store.reviewed_set("scope_a", &files);
        let set_b = store.reviewed_set("scope_b", &files);

        assert!(set_a.contains(&0));
        assert!(set_b.is_empty());
    }

    #[test]
    fn repo_id_deterministic() {
        let id1 = repo_id(Path::new("/tmp/test-repo"));
        let id2 = repo_id(Path::new("/tmp/test-repo"));
        assert_eq!(id1, id2);

        let id3 = repo_id(Path::new("/tmp/other-repo"));
        assert_ne!(id1, id3);
    }
}
