//! Post-diff safety checks: validates file counts, paths, and status consistency.

use crate::difft;
use crate::git::ChangedEntry;
use std::collections::HashSet;
use std::path::Path;

/// Runs post-diff safety checks. Returns Ok(()) or an error message.
pub fn verify(
    expected: &[ChangedEntry],
    actual: &[difft::DifftFile],
) -> Result<(), String> {
    // 1. Count check
    if actual.len() != expected.len() {
        return Err(format!(
            "Integrity: git reported {} files but diff produced {}",
            expected.len(),
            actual.len()
        ));
    }

    // 2. Path check
    let output_paths: HashSet<&Path> = actual.iter().map(|f| f.path.as_path()).collect();
    for entry in expected {
        if !output_paths.contains(entry.new_path.as_path()) {
            return Err(format!(
                "Integrity: file {} reported by git but missing from diff output",
                entry.new_path.display()
            ));
        }
    }

    // 3. Status contradiction check
    for entry in expected {
        if let Some(file) = actual.iter().find(|f| f.path == entry.new_path) {
            if entry.status.starts_with('A') && file.status == difft::Status::Deleted {
                return Err(format!(
                    "Integrity: git says {} is Added but difft says Deleted",
                    entry.new_path.display()
                ));
            }
            if entry.status.starts_with('D') && file.status == difft::Status::Created {
                return Err(format!(
                    "Integrity: git says {} is Deleted but difft says Created",
                    entry.new_path.display()
                ));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn entry(status: &str, path: &str) -> ChangedEntry {
        ChangedEntry {
            status: status.to_string(),
            old_path: PathBuf::from(path),
            new_path: PathBuf::from(path),
        }
    }

    fn difft_file(path: &str, status: difft::Status) -> difft::DifftFile {
        difft::DifftFile {
            path: PathBuf::from(path),
            language: "Rust".to_string(),
            status,
            aligned_lines: vec![],
            chunks: vec![],
        }
    }

    #[test]
    fn verify_matching() {
        let expected = vec![entry("M", "src/lib.rs")];
        let actual = vec![difft_file("src/lib.rs", difft::Status::Changed)];
        assert!(verify(&expected, &actual).is_ok());
    }

    #[test]
    fn verify_count_mismatch() {
        let expected = vec![entry("M", "a.rs"), entry("M", "b.rs")];
        let actual = vec![difft_file("a.rs", difft::Status::Changed)];
        assert!(verify(&expected, &actual).unwrap_err().contains("2 files but diff produced 1"));
    }

    #[test]
    fn verify_missing_path() {
        let expected = vec![entry("M", "a.rs"), entry("M", "b.rs")];
        let actual = vec![
            difft_file("a.rs", difft::Status::Changed),
            difft_file("c.rs", difft::Status::Changed),
        ];
        assert!(verify(&expected, &actual).unwrap_err().contains("b.rs"));
    }

    #[test]
    fn verify_status_contradiction_added_deleted() {
        let expected = vec![entry("A", "new.rs")];
        let actual = vec![difft_file("new.rs", difft::Status::Deleted)];
        assert!(verify(&expected, &actual).unwrap_err().contains("Added but difft says Deleted"));
    }

    #[test]
    fn verify_status_contradiction_deleted_created() {
        let expected = vec![entry("D", "old.rs")];
        let actual = vec![difft_file("old.rs", difft::Status::Created)];
        assert!(verify(&expected, &actual).unwrap_err().contains("Deleted but difft says Created"));
    }
}
