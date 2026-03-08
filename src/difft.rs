//! Difftastic JSON output parsing and type definitions.

use serde::Deserialize;
use std::path::PathBuf;

use crate::types::SyntaxHighlight;

/// File status from difftastic JSON.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Unchanged,
    Created,
    Deleted,
    Changed,
}

/// A file entry from difftastic's JSON output.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct DifftFile {
    pub path: PathBuf,
    pub language: String,
    pub status: Status,
    #[serde(default)]
    pub aligned_lines: Vec<(Option<u32>, Option<u32>)>,
    #[serde(default)]
    pub chunks: Vec<Chunk>,
}

/// A chunk (hunk) of changes within a file.
pub type Chunk = Vec<DiffLine>;

/// A single diff line entry.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct DiffLine {
    pub lhs: Option<Side>,
    pub rhs: Option<Side>,
}

/// One side of a diff line.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Side {
    pub line_number: u32,
    pub changes: Vec<Change>,
}

/// A specific change region within a line.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Change {
    pub start: u32,
    #[serde(rename = "end")]
    pub end_col: u32,
    pub content: String,
    pub highlight: Highlight,
}

/// Token highlight categories from difftastic JSON.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Highlight {
    Delimiter,
    Normal,
    String,
    Type,
    Comment,
    Keyword,
    TreeSitterError,
}

impl Highlight {
    pub fn to_syntax_highlight(self) -> Option<SyntaxHighlight> {
        match self {
            Highlight::Delimiter => Some(SyntaxHighlight::Delimiter),
            Highlight::Normal => Some(SyntaxHighlight::Normal),
            Highlight::String => Some(SyntaxHighlight::String),
            Highlight::Type => Some(SyntaxHighlight::Type),
            Highlight::Comment => Some(SyntaxHighlight::Comment),
            Highlight::Keyword => Some(SyntaxHighlight::Keyword),
            Highlight::TreeSitterError => None,
        }
    }
}

/// Parses difftastic JSON output.
pub fn parse(json: &str) -> Result<Vec<DifftFile>, serde_json::Error> {
    if let Ok(files) = serde_json::from_str::<Vec<DifftFile>>(json) {
        return Ok(files);
    }

    json.lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_array() {
        let files = parse("[]").unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn parse_created_file() {
        let json = r#"[{
            "path": "src/new.rs",
            "language": "Rust",
            "status": "created",
            "chunks": []
        }]"#;
        let files = parse(json).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, Status::Created);
    }

    #[test]
    fn parse_modified_file_with_changes() {
        let json = r#"[{
            "path": "src/lib.rs",
            "language": "Rust",
            "status": "changed",
            "chunks": [[
                {
                    "lhs": {"line_number": 5, "changes": [{"start": 0, "end": 10, "content": "old_code", "highlight": "normal"}]},
                    "rhs": {"line_number": 5, "changes": [{"start": 0, "end": 12, "content": "new_code_!!", "highlight": "normal"}]}
                }
            ]]
        }]"#;
        let files = parse(json).unwrap();
        assert_eq!(files[0].chunks.len(), 1);
        assert_eq!(files[0].chunks[0][0].lhs.as_ref().unwrap().changes[0].end_col, 10);
    }

    #[test]
    fn parse_newline_separated_objects() {
        let json = r#"{"path":"a.rs","language":"Rust","status":"changed","chunks":[]}
{"path":"b.rs","language":"Rust","status":"created","chunks":[]}"#;
        let files = parse(json).unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn parse_with_aligned_lines() {
        let json = r#"[{
            "path": "src/lib.rs",
            "language": "Rust",
            "status": "changed",
            "aligned_lines": [[0, 0], [1, null], [2, 1]],
            "chunks": []
        }]"#;
        let files = parse(json).unwrap();
        assert_eq!(files[0].aligned_lines[1], (Some(1), None));
    }

    #[test]
    fn parse_highlight_types() {
        let json = r#"[{
            "path": "src/lib.rs",
            "language": "Rust",
            "status": "changed",
            "chunks": [[
                {
                    "rhs": {
                        "line_number": 5,
                        "changes": [
                            {"start": 0, "end": 3, "content": "let", "highlight": "keyword"},
                            {"start": 4, "end": 7, "content": "foo", "highlight": "normal"},
                            {"start": 10, "end": 13, "content": "\"bar\"", "highlight": "string"}
                        ]
                    }
                }
            ]]
        }]"#;
        let files = parse(json).unwrap();
        let rhs = files[0].chunks[0][0].rhs.as_ref().unwrap();
        assert_eq!(rhs.changes[0].highlight, Highlight::Keyword);
        assert_eq!(rhs.changes[2].highlight, Highlight::String);
    }
}
