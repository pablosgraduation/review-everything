//! Post-diff safety checks: validates file counts, paths, and status consistency.

use crate::difft;
use crate::git::ChangedEntry;
use crate::types::{DisplayFile, FileStatus};
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

/// Post-processing integrity checks on display-ready files.
/// Catches bugs in our pipeline — these should never fire in normal operation.
pub fn verify_display(files: &[DisplayFile]) -> Result<(), String> {
    // 1. No duplicate paths
    let mut seen = HashSet::with_capacity(files.len());
    for f in files {
        if !seen.insert(&f.path) {
            return Err(format!(
                "Integrity: duplicate path {}",
                f.path.display()
            ));
        }
    }

    // 2. No Unchanged files
    for f in files {
        if f.status == FileStatus::Unchanged {
            return Err(format!(
                "Integrity: unchanged file {} should have been filtered",
                f.path.display()
            ));
        }
    }

    for f in files {
        // 3. Hunk structural validity
        for (i, &(start, end, _)) in f.hunks.iter().enumerate() {
            if start >= end {
                return Err(format!(
                    "Integrity: {}: hunk {} has start ({}) >= end ({})",
                    f.path.display(),
                    i,
                    start,
                    end
                ));
            }
            if end > f.rows.len() as u32 {
                return Err(format!(
                    "Integrity: {}: hunk {} end ({}) > rows.len() ({})",
                    f.path.display(),
                    i,
                    end,
                    f.rows.len()
                ));
            }
            if i > 0 {
                let prev_end = f.hunks[i - 1].1;
                if start < prev_end {
                    return Err(format!(
                        "Integrity: {}: hunk {} start ({}) < previous hunk end ({})",
                        f.path.display(),
                        i,
                        start,
                        prev_end
                    ));
                }
            }
        }

        // 4. aligned_lines.len() == rows.len()
        if f.aligned_lines.len() != f.rows.len() {
            return Err(format!(
                "Integrity: {}: aligned_lines.len() ({}) != rows.len() ({})",
                f.path.display(),
                f.aligned_lines.len(),
                f.rows.len()
            ));
        }

        // 5. Filler consistency: filler sides must have empty content
        for (ri, row) in f.rows.iter().enumerate() {
            if row.left.is_filler && !row.left.content.is_empty() {
                return Err(format!(
                    "Integrity: {}: row {} left is filler but has content",
                    f.path.display(),
                    ri
                ));
            }
            if row.right.is_filler && !row.right.content.is_empty() {
                return Err(format!(
                    "Integrity: {}: row {} right is filler but has content",
                    f.path.display(),
                    ri
                ));
            }
        }

        // 6. Created file (without moved_from): left side should all be filler
        if f.status == FileStatus::Created && f.moved_from.is_none() {
            for (ri, row) in f.rows.iter().enumerate() {
                if !row.left.is_filler {
                    return Err(format!(
                        "Integrity: {}: created (non-rename) file has non-filler left at row {}",
                        f.path.display(),
                        ri
                    ));
                }
            }
        }

        // 7. Deleted file: right side should all be filler
        if f.status == FileStatus::Deleted {
            for (ri, row) in f.rows.iter().enumerate() {
                if !row.right.is_filler {
                    return Err(format!(
                        "Integrity: {}: deleted file has non-filler right at row {}",
                        f.path.display(),
                        ri
                    ));
                }
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

    // --- verify_display tests ---

    use crate::types::{HunkKind, Row, Side};

    fn display_file(path: &str, status: FileStatus) -> DisplayFile {
        DisplayFile {
            path: PathBuf::from(path),
            moved_from: None,
            language: "Rust".to_string(),
            status,
            additions: 0,
            deletions: 0,
            rows: vec![],
            hunks: vec![],
            aligned_lines: vec![],
            content_hash: 0,
        }
    }

    fn modified_row() -> Row {
        Row {
            left: Side::new("old".to_string(), false, Default::default()),
            right: Side::new("new".to_string(), false, Default::default()),
        }
    }

    fn created_row() -> Row {
        Row {
            left: Side::filler(),
            right: Side::new("new".to_string(), false, Default::default()),
        }
    }

    fn deleted_row() -> Row {
        Row {
            left: Side::new("old".to_string(), false, Default::default()),
            right: Side::filler(),
        }
    }

    #[test]
    fn display_valid_files_pass() {
        let mut f = display_file("a.rs", FileStatus::Modified);
        f.rows = vec![modified_row()];
        f.aligned_lines = vec![(Some(1), Some(1))];
        f.hunks = vec![(0, 1, HunkKind::Mixed)];
        assert!(verify_display(&[f]).is_ok());
    }

    #[test]
    fn display_duplicate_path() {
        let a = display_file("a.rs", FileStatus::Modified);
        let b = display_file("a.rs", FileStatus::Modified);
        assert!(verify_display(&[a, b]).unwrap_err().contains("duplicate path"));
    }

    #[test]
    fn display_unchanged_file() {
        let f = display_file("a.rs", FileStatus::Unchanged);
        assert!(verify_display(&[f]).unwrap_err().contains("unchanged file"));
    }

    #[test]
    fn display_hunk_start_ge_end() {
        let mut f = display_file("a.rs", FileStatus::Modified);
        f.rows = vec![modified_row()];
        f.aligned_lines = vec![(Some(1), Some(1))];
        f.hunks = vec![(1, 1, HunkKind::Mixed)];
        assert!(verify_display(&[f]).unwrap_err().contains("start (1) >= end (1)"));
    }

    #[test]
    fn display_hunk_out_of_bounds() {
        let mut f = display_file("a.rs", FileStatus::Modified);
        f.rows = vec![modified_row()];
        f.aligned_lines = vec![(Some(1), Some(1))];
        f.hunks = vec![(0, 5, HunkKind::Mixed)];
        assert!(verify_display(&[f]).unwrap_err().contains("end (5) > rows.len() (1)"));
    }

    #[test]
    fn display_overlapping_hunks() {
        let mut f = display_file("a.rs", FileStatus::Modified);
        f.rows = vec![modified_row(), modified_row(), modified_row()];
        f.aligned_lines = vec![(Some(1), Some(1)), (Some(2), Some(2)), (Some(3), Some(3))];
        f.hunks = vec![(0, 2, HunkKind::Mixed), (1, 3, HunkKind::Mixed)];
        assert!(verify_display(&[f]).unwrap_err().contains("start (1) < previous hunk end (2)"));
    }

    #[test]
    fn display_aligned_lines_mismatch() {
        let mut f = display_file("a.rs", FileStatus::Modified);
        f.rows = vec![modified_row()];
        f.aligned_lines = vec![(Some(1), Some(1)), (Some(2), Some(2))];
        assert!(verify_display(&[f]).unwrap_err().contains("aligned_lines.len() (2) != rows.len() (1)"));
    }

    #[test]
    fn display_filler_with_content() {
        let mut f = display_file("a.rs", FileStatus::Modified);
        f.rows = vec![Row {
            left: Side::new("oops".to_string(), true, Default::default()),
            right: Side::new("new".to_string(), false, Default::default()),
        }];
        f.aligned_lines = vec![(None, Some(1))];
        assert!(verify_display(&[f]).unwrap_err().contains("filler but has content"));
    }

    #[test]
    fn display_created_non_rename_with_old_content() {
        let mut f = display_file("a.rs", FileStatus::Created);
        f.rows = vec![modified_row()]; // left side is non-filler
        f.aligned_lines = vec![(Some(1), Some(1))];
        assert!(verify_display(&[f]).unwrap_err().contains("created (non-rename)"));
    }

    #[test]
    fn display_deleted_with_new_content() {
        let mut f = display_file("a.rs", FileStatus::Deleted);
        f.rows = vec![modified_row()]; // right side is non-filler
        f.aligned_lines = vec![(Some(1), Some(1))];
        assert!(verify_display(&[f]).unwrap_err().contains("deleted file has non-filler right"));
    }

    #[test]
    fn display_created_rename_allows_old_content() {
        let mut f = display_file("b.rs", FileStatus::Created);
        f.moved_from = Some(PathBuf::from("a.rs"));
        f.rows = vec![modified_row()]; // left side has content — valid for renames
        f.aligned_lines = vec![(Some(1), Some(1))];
        assert!(verify_display(&[f]).is_ok());
    }

    #[test]
    fn display_valid_created_file() {
        let mut f = display_file("new.rs", FileStatus::Created);
        f.rows = vec![created_row()];
        f.aligned_lines = vec![(None, Some(1))];
        f.hunks = vec![(0, 1, HunkKind::AddOnly)];
        assert!(verify_display(&[f]).is_ok());
    }

    #[test]
    fn display_valid_deleted_file() {
        let mut f = display_file("old.rs", FileStatus::Deleted);
        f.rows = vec![deleted_row()];
        f.aligned_lines = vec![(Some(1), None)];
        f.hunks = vec![(0, 1, HunkKind::DeleteOnly)];
        assert!(verify_display(&[f]).is_ok());
    }
}
