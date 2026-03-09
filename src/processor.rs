//! Transforms parsed difftastic data into aligned display rows with highlights.

use crate::difft::{Change, Chunk, DifftFile, Highlight, Status};
use crate::types::*;
use smallvec::SmallVec;
use std::collections::HashMap;

/// Processes a difftastic file into display-ready format.
pub fn process_file(
    file: DifftFile,
    old_lines: Vec<String>,
    new_lines: Vec<String>,
    stats: Option<(u32, u32)>,
    content_hash: u64,
) -> DisplayFile {
    match file.status {
        Status::Created => process_created(file, new_lines, stats, content_hash),
        Status::Deleted => process_deleted(file, old_lines, stats, content_hash),
        Status::Changed | Status::Unchanged => process_changed(file, &old_lines, &new_lines, stats, content_hash),
    }
}

fn process_created(file: DifftFile, new_lines: Vec<String>, stats: Option<(u32, u32)>, content_hash: u64) -> DisplayFile {
    let num_lines = new_lines.len();
    let rows: Vec<Row> = new_lines
        .into_iter()
        .map(|line| Row {
            left: Side::filler(),
            right: Side::new(line, false, Highlights::new()),
        })
        .collect();

    let aligned_lines: Vec<(Option<u32>, Option<u32>)> =
        (0..num_lines).map(|i| (None, Some(i as u32))).collect();

    let (additions, deletions) = stats.unwrap_or((rows.len() as u32, 0));
    let hunks = if rows.is_empty() { vec![] } else { vec![(0, rows.len() as u32, HunkKind::AddOnly)] };

    DisplayFile {
        path: file.path,
        moved_from: None,
        language: file.language,
        status: FileStatus::Created,
        additions,
        deletions,
        rows,
        hunks,
        aligned_lines,
        content_hash,
    }
}

fn process_deleted(file: DifftFile, old_lines: Vec<String>, stats: Option<(u32, u32)>, content_hash: u64) -> DisplayFile {
    let num_lines = old_lines.len();
    let rows: Vec<Row> = old_lines
        .into_iter()
        .map(|line| Row {
            left: Side::with_full_highlight(line),
            right: Side::filler(),
        })
        .collect();

    let aligned_lines: Vec<(Option<u32>, Option<u32>)> =
        (0..num_lines).map(|i| (Some(i as u32), None)).collect();

    let (additions, deletions) = stats.unwrap_or((0, rows.len() as u32));
    let hunks = if rows.is_empty() { vec![] } else { vec![(0, rows.len() as u32, HunkKind::DeleteOnly)] };

    DisplayFile {
        path: file.path,
        moved_from: None,
        language: file.language,
        status: FileStatus::Deleted,
        additions,
        deletions,
        rows,
        hunks,
        aligned_lines,
        content_hash,
    }
}

type ChangeInfo<'a> = &'a [Change];

fn extract_changes(
    chunks: &[Chunk],
) -> (HashMap<u32, ChangeInfo<'_>>, HashMap<u32, ChangeInfo<'_>>) {
    let capacity: usize = chunks.iter().map(|c| c.len()).sum();
    let mut lhs_changes: HashMap<u32, ChangeInfo<'_>> = HashMap::with_capacity(capacity);
    let mut rhs_changes: HashMap<u32, ChangeInfo<'_>> = HashMap::with_capacity(capacity);

    for chunk in chunks {
        for diff_line in chunk {
            if let Some(side) = &diff_line.lhs {
                lhs_changes.insert(side.line_number, &side.changes);
            }
            if let Some(side) = &diff_line.rhs {
                rhs_changes.insert(side.line_number, &side.changes);
            }
        }
    }

    (lhs_changes, rhs_changes)
}

fn process_changed(
    file: DifftFile,
    old_lines: &[String],
    new_lines: &[String],
    stats: Option<(u32, u32)>,
    content_hash: u64,
) -> DisplayFile {
    let (lhs_changes, rhs_changes) = extract_changes(&file.chunks);
    let num_rows = file.aligned_lines.len();

    let mut rows = Vec::with_capacity(num_rows);
    let mut hunks: Vec<(u32, u32, HunkKind)> = Vec::new();
    let mut in_hunk = false;
    let mut hunk_start = 0u32;
    let mut hunk_has_left = false;
    let mut hunk_has_right = false;

    for (row_idx, (lhs_ln, rhs_ln)) in file.aligned_lines.iter().enumerate() {
        let left_content = lhs_ln
            .and_then(|ln| old_lines.get(ln as usize))
            .cloned()
            .unwrap_or_default();
        let right_content = rhs_ln
            .and_then(|ln| new_lines.get(ln as usize))
            .cloned()
            .unwrap_or_default();

        let left_changes = lhs_ln.and_then(|ln| lhs_changes.get(&ln).copied());
        let right_changes = rhs_ln.and_then(|ln| rhs_changes.get(&ln).copied());

        let left_highlights = left_changes
            .map(|changes| compute_highlights(&left_content, changes))
            .unwrap_or_default();
        let right_highlights = right_changes
            .map(|changes| compute_highlights(&right_content, changes))
            .unwrap_or_default();

        let is_changed = lhs_ln.is_none()
            || rhs_ln.is_none()
            || !left_highlights.is_empty()
            || !right_highlights.is_empty();

        if is_changed && !in_hunk {
            hunk_start = row_idx as u32;
            in_hunk = true;
            hunk_has_left = false;
            hunk_has_right = false;
        }

        if is_changed && in_hunk {
            if rhs_ln.is_none() || !left_highlights.is_empty() {
                hunk_has_left = true;
            }
            if lhs_ln.is_none() || !right_highlights.is_empty() {
                hunk_has_right = true;
            }
        }

        if !is_changed && in_hunk {
            hunks.push((hunk_start, row_idx as u32, classify_hunk(hunk_has_left, hunk_has_right)));
            in_hunk = false;
        }

        rows.push(Row {
            left: Side::new(left_content, lhs_ln.is_none(), left_highlights),
            right: Side::new(right_content, rhs_ln.is_none(), right_highlights),
        });
    }

    // Close final hunk if file ends mid-hunk
    if in_hunk {
        hunks.push((hunk_start, rows.len() as u32, classify_hunk(hunk_has_left, hunk_has_right)));
    }

    let status = if file.status == Status::Unchanged {
        FileStatus::Unchanged
    } else {
        FileStatus::Modified
    };

    let (additions, deletions) = stats.unwrap_or((0, 0));

    DisplayFile {
        path: file.path,
        moved_from: None,
        language: file.language,
        status,
        additions,
        deletions,
        rows,
        hunks,
        aligned_lines: file.aligned_lines,
        content_hash,
    }
}

fn classify_hunk(has_left: bool, has_right: bool) -> HunkKind {
    match (has_left, has_right) {
        (true, true) => HunkKind::Mixed,
        (true, false) => HunkKind::DeleteOnly,
        (false, true) => HunkKind::AddOnly,
        (false, false) => HunkKind::Mixed, // shouldn't happen
    }
}

/// Computes highlight regions for a line based on its changes.
fn compute_highlights(content: &str, changes: &[Change]) -> Highlights {
    if changes.is_empty() {
        return Highlights::new();
    }

    let len = content.len() as u32;
    if changes.len() == 1 && changes[0].start == 0 && changes[0].end_col >= len {
        return smallvec::smallvec![HighlightRegion::full_line()];
    }

    let mut regions: SmallVec<[(u32, u32, Option<Highlight>); 4]> = changes
        .iter()
        .map(|c| (c.start, c.end_col, Some(c.highlight)))
        .collect();
    regions.sort_unstable_by_key(|r| r.0);

    let merged = merge_regions(&regions, content.as_bytes());

    if covers_all_non_whitespace(content, &merged) {
        return smallvec::smallvec![HighlightRegion::full_line()];
    }

    merged
        .into_iter()
        .map(|(start, end, highlight)| {
            let mut r = HighlightRegion::columns(start, end);
            r.highlight = highlight.and_then(|h| h.to_syntax_highlight());
            r
        })
        .collect()
}

fn merge_regions(
    regions: &[(u32, u32, Option<Highlight>)],
    bytes: &[u8],
) -> SmallVec<[(u32, u32, Option<Highlight>); 4]> {
    let mut merged: SmallVec<[(u32, u32, Option<Highlight>); 4]> =
        SmallVec::with_capacity(regions.len());

    for &(start, end, highlight) in regions {
        if let Some((_, last_end, _)) = merged.last_mut() {
            let gap_start = *last_end as usize;
            let gap_end = start as usize;

            if gap_start >= gap_end || is_whitespace_only(bytes, gap_start, gap_end) {
                *last_end = (*last_end).max(end);
                continue;
            }
        }
        merged.push((start, end, highlight));
    }

    merged
}

#[inline]
fn is_whitespace_only(bytes: &[u8], start: usize, end: usize) -> bool {
    bytes
        .get(start..end)
        .is_some_and(|slice| slice.iter().all(u8::is_ascii_whitespace))
}

fn covers_all_non_whitespace(line: &str, regions: &[(u32, u32, Option<Highlight>)]) -> bool {
    let mut has_non_ws = false;

    for (i, c) in line.char_indices() {
        if !c.is_whitespace() {
            has_non_ws = true;
            let pos = i as u32;
            if !regions.iter().any(|(start, end, _)| pos >= *start && pos < *end) {
                return false;
            }
        }
    }

    has_non_ws
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::difft::{DiffLine, Side as DiffSide};

    fn change(start: u32, end: u32) -> Change {
        Change {
            start,
            end_col: end,
            content: String::new(),
            highlight: Highlight::Normal,
        }
    }

    fn diff_side(line: u32, changes: Vec<Change>) -> DiffSide {
        DiffSide {
            line_number: line,
            changes,
        }
    }

    #[test]
    fn created_file_all_additions() {
        let file = DifftFile {
            path: "new.rs".into(),
            language: "Rust".into(),
            status: Status::Created,
            aligned_lines: vec![],
            chunks: vec![],
        };
        let result = process_file(file, vec![], vec!["a".into(), "b".into()], Some((2, 0)), 0);
        assert_eq!(result.rows.len(), 2);
        assert!(result.rows[0].left.is_filler);
        assert_eq!(result.rows[0].right.content, "a");
        assert_eq!(result.additions, 2);
    }

    #[test]
    fn deleted_file_all_deletions() {
        let file = DifftFile {
            path: "old.rs".into(),
            language: "Rust".into(),
            status: Status::Deleted,
            aligned_lines: vec![],
            chunks: vec![],
        };
        let result = process_file(file, vec!["x".into(), "y".into()], vec![], Some((0, 2)), 0);
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.rows[0].left.content, "x");
        assert!(result.rows[0].right.is_filler);
    }

    #[test]
    fn modification_with_aligned_lines() {
        let file = DifftFile {
            path: "mod.rs".into(),
            language: "Rust".into(),
            status: Status::Changed,
            aligned_lines: vec![(Some(0), Some(0)), (Some(1), Some(1)), (Some(2), Some(2))],
            chunks: vec![vec![DiffLine {
                lhs: Some(diff_side(1, vec![change(0, 3)])),
                rhs: Some(diff_side(1, vec![change(0, 6)])),
            }]],
        };
        let result = process_file(
            file,
            vec!["line1".into(), "foo".into(), "line3".into()],
            vec!["line1".into(), "foobar".into(), "line3".into()],
            Some((1, 1)),
            0,
        );
        assert_eq!(result.rows.len(), 3);
        assert!(!result.rows[1].left.highlights.is_empty());
    }

    #[test]
    fn hunk_starts_detected() {
        let file = DifftFile {
            path: "hunks.rs".into(),
            language: "Rust".into(),
            status: Status::Changed,
            aligned_lines: vec![
                (Some(0), Some(0)),
                (Some(1), Some(1)),
                (Some(2), Some(2)),
                (Some(3), Some(3)),
                (None, Some(4)),
            ],
            chunks: vec![
                vec![DiffLine {
                    lhs: Some(diff_side(1, vec![change(0, 3)])),
                    rhs: Some(diff_side(1, vec![change(0, 3)])),
                }],
                vec![DiffLine {
                    lhs: None,
                    rhs: Some(diff_side(4, vec![change(0, 5)])),
                }],
            ],
        };
        let result = process_file(
            file,
            vec!["a".into(), "b".into(), "c".into(), "d".into()],
            vec!["a".into(), "B".into(), "c".into(), "d".into(), "e".into()],
            None,
            0,
        );
        assert_eq!(result.hunks.len(), 2);
        assert_eq!(result.hunks[0].0, 1);  // start
        assert_eq!(result.hunks[0].1, 2);  // end (exclusive)
        assert_eq!(result.hunks[0].2, HunkKind::Mixed);
        assert_eq!(result.hunks[1].0, 4);  // start
        assert_eq!(result.hunks[1].1, 5);  // end (exclusive, last row)
        assert_eq!(result.hunks[1].2, HunkKind::AddOnly);
    }

    #[test]
    fn highlight_full_line() {
        let highlights = compute_highlights("hello", &[change(0, 5)]);
        assert!(highlights[0].end == FULL_LINE);
    }

    #[test]
    fn highlight_partial() {
        let highlights = compute_highlights("hello world", &[change(0, 5)]);
        assert_eq!(highlights[0].start, 0);
        assert_eq!(highlights[0].end, 5);
    }

    #[test]
    fn highlight_merges_across_whitespace() {
        let highlights = compute_highlights("foo bar", &[change(0, 3), change(4, 7)]);
        assert_eq!(highlights.len(), 1);
        assert!(highlights[0].end == FULL_LINE);
    }
}
