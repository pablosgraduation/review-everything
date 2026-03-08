//! Core data types for the diff viewer: rows, sides, highlights, and file metadata.

use smallvec::SmallVec;
use std::path::PathBuf;

/// Most lines have 0-2 highlight regions; inline storage avoids heap allocation.
pub type Highlights = SmallVec<[HighlightRegion; 2]>;

/// A highlight region within a line, specified by column range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightRegion {
    /// Start column (0-indexed, inclusive).
    pub start: u32,
    /// End column (exclusive), or `FULL_LINE` to indicate full-line highlight.
    pub end: i32,
    /// Syntax highlight type from difftastic (for foreground coloring).
    pub highlight: Option<SyntaxHighlight>,
}

pub const FULL_LINE: i32 = -1;

/// Syntax highlight categories from difftastic's tree-sitter analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxHighlight {
    Delimiter,
    Normal,
    String,
    Type,
    Comment,
    Keyword,
}

impl HighlightRegion {
    #[inline]
    pub fn full_line() -> Self {
        Self {
            start: 0,
            end: FULL_LINE,
            highlight: None,
        }
    }

    #[inline]
    pub fn columns(start: u32, end: u32) -> Self {
        Self {
            start,
            end: i32::try_from(end).unwrap_or(i32::MAX),
            highlight: None,
        }
    }
}

/// One side (left or right) of a diff row for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Side {
    pub content: String,
    pub is_filler: bool,
    pub highlights: Highlights,
}

impl Side {
    #[inline]
    pub fn new(content: String, is_filler: bool, highlights: Highlights) -> Self {
        Self {
            content,
            is_filler,
            highlights,
        }
    }

    #[inline]
    pub fn filler() -> Self {
        Self::new(String::new(), true, Highlights::new())
    }

    #[inline]
    pub fn with_full_highlight(content: String) -> Self {
        Self::new(
            content,
            false,
            smallvec::smallvec![HighlightRegion::full_line()],
        )
    }
}

/// A single row in the diff display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub left: Side,
    pub right: Side,
}

/// What kind of change a hunk represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HunkKind {
    /// Only additions (filler on left, content on right).
    AddOnly,
    /// Only deletions (content on left, filler on right).
    DeleteOnly,
    /// Both sides have changes.
    Mixed,
}

/// File status from git/difft.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Created,
    Deleted,
    Modified,
    Unchanged,
}

impl std::fmt::Display for FileStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileStatus::Created => write!(f, "created"),
            FileStatus::Deleted => write!(f, "deleted"),
            FileStatus::Modified => write!(f, "modified"),
            FileStatus::Unchanged => write!(f, "unchanged"),
        }
    }
}

/// A processed file ready for display in the diff viewer.
#[derive(Debug, Clone)]
pub struct DisplayFile {
    pub path: PathBuf,
    pub moved_from: Option<PathBuf>,
    #[allow(dead_code)] // used by difft JSON parsing; reserved for future syntax theme support
    pub language: String,
    pub status: FileStatus,
    pub additions: u32,
    pub deletions: u32,
    pub rows: Vec<Row>,
    /// Hunk ranges: (start_row, end_row_exclusive, kind).
    pub hunks: Vec<(u32, u32, HunkKind)>,
    /// Original line number mapping per display row.
    pub aligned_lines: Vec<(Option<u32>, Option<u32>)>,
}
