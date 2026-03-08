//! Git command wrappers: log, diff, file content, and status queries.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Entry from `git diff --name-status` output.
#[derive(Debug, Clone)]
pub struct ChangedEntry {
    pub status: String,
    pub old_path: PathBuf,
    pub new_path: PathBuf,
}

/// Stats for files: (additions, deletions) keyed by path.
pub type FileStats = HashMap<PathBuf, (u32, u32)>;

/// Git log entry.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub full_hash: String,
    pub short_hash: String,
    pub date: String,
    pub subject: String,
}

/// Gets the git repository root directory.
pub fn git_root() -> Result<PathBuf, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|e| format!("Failed to run git: {e}"))?;

    if !output.status.success() {
        return Err("Not a git repository".to_string());
    }

    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim(),
    ))
}

/// Fetches file content from git at a specific commit.
pub fn git_file_content(commit: &str, path: &Path) -> Option<String> {
    Command::new("git")
        .arg("show")
        .arg(format!("{commit}:{}", path.display()))
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Fetches file content from git index (staged version).
pub fn git_index_content(path: &Path) -> Option<String> {
    Command::new("git")
        .arg("show")
        .arg(format!(":{}", path.display()))
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Gets the full list of changed files from `git diff --name-status`.
pub fn git_changed_files(extra_args: &[&str]) -> Result<Vec<ChangedEntry>, String> {
    let mut args = vec!["diff", "--name-status"];
    args.extend(extra_args);

    let output = Command::new("git")
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to run git: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git name-status failed: {stderr}"));
    }

    Ok(parse_name_status_entries(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

fn parse_name_status_entries(output: &str) -> Vec<ChangedEntry> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let status = parts.next()?.trim().to_string();
            let first_path = PathBuf::from(parts.next()?.trim());
            let second_path = parts.next().map(|p| PathBuf::from(p.trim()));

            if status.starts_with('R') || status.starts_with('C') {
                Some(ChangedEntry {
                    status,
                    old_path: first_path,
                    new_path: second_path?,
                })
            } else {
                Some(ChangedEntry {
                    status,
                    old_path: first_path.clone(),
                    new_path: first_path,
                })
            }
        })
        .collect()
}

/// Gets diff stats from git using `--numstat`.
pub fn git_diff_stats(extra_args: &[&str]) -> FileStats {
    let mut args = vec!["diff", "--numstat"];
    args.extend(extra_args);

    let output = Command::new("git").args(&args).output().ok();

    let Some(output) = output.filter(|o| o.status.success()) else {
        return HashMap::new();
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let add = parts.next()?.parse().ok()?;
            let del = parts.next()?.parse().ok()?;
            let path = parts.next()?;
            Some((PathBuf::from(path), (add, del)))
        })
        .collect()
}

/// Gets rename mapping from `git diff --name-status -M`.
pub fn git_rename_map(extra_args: &[&str]) -> HashMap<PathBuf, PathBuf> {
    let mut cmd = Command::new("git");
    cmd.args(["diff", "--name-status", "-M"]);
    cmd.args(extra_args);

    let output = cmd.output().ok();
    let Some(output) = output.filter(|o| o.status.success()) else {
        return HashMap::new();
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut parts = line.trim().split('\t');
            let status = parts.next()?;
            if !status.starts_with('R') {
                return None;
            }
            let old_path = parts.next()?.trim();
            let new_path = parts.next()?.trim();
            if old_path.is_empty() || new_path.is_empty() {
                return None;
            }
            Some((PathBuf::from(new_path), PathBuf::from(old_path)))
        })
        .collect()
}

/// Gets untracked files from git.
pub fn git_untracked_files() -> Vec<PathBuf> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "--others",
            "--exclude-standard",
            "--full-name",
            ":/",
        ])
        .output()
        .ok();

    let Some(output) = output.filter(|o| o.status.success()) else {
        return Vec::new();
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Parses a git commit range into `(old_commit, new_commit)`.
pub fn parse_git_range(range: &str) -> (String, String) {
    if let Some((a, b)) = range.split_once("...") {
        let base = git_merge_base(a, b).unwrap_or_else(|| format!("{a}^"));
        (base, b.to_string())
    } else if let Some((old, new)) = range.split_once("..") {
        (old.to_string(), new.to_string())
    } else {
        (format!("{range}^"), range.to_string())
    }
}

fn git_merge_base(a: &str, b: &str) -> Option<String> {
    Command::new("git")
        .args(["merge-base", a, b])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Gets git log entries.
pub fn git_log(limit: usize) -> Result<Vec<LogEntry>, String> {
    git_log_revspec(limit, None)
}

/// Load git log entries, optionally scoped to ancestors of `revspec`.
pub fn git_log_revspec(limit: usize, revspec: Option<&str>) -> Result<Vec<LogEntry>, String> {
    let mut cmd = Command::new("git");
    cmd.args([
        "log",
        &format!("-n{limit}"),
        "--pretty=format:%H\t%h\t%ad\t%s",
        "--date=format:%Y-%m-%d %H:%M",
    ]);
    if let Some(rev) = revspec {
        cmd.arg(rev);
    }
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to run git log: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git log failed: {stderr}"));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(4, '\t');
            let full = parts.next()?.to_string();
            let short = parts.next()?.to_string();
            let date = parts.next()?.to_string();
            let subject = parts.next().unwrap_or("").to_string();
            Some(LogEntry {
                full_hash: full,
                short_hash: short,
                date,
                subject,
            })
        })
        .collect())
}

/// Checks if there are staged changes.
pub fn has_staged_changes() -> bool {
    Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .status()
        .map(|s| !s.success())
        .unwrap_or(false)
}

/// Checks if there are unstaged changes (tracked or untracked).
pub fn has_unstaged_changes() -> bool {
    let has_tracked = Command::new("git")
        .args(["diff", "--quiet"])
        .status()
        .map(|s| !s.success())
        .unwrap_or(false);

    if has_tracked {
        return true;
    }

    let untracked = Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard", ":/"])
        .output()
        .ok();

    untracked
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .any(|l| !l.is_empty())
        })
        .unwrap_or(false)
}

/// Reads a file from the working tree relative to git root.
pub fn working_tree_content(path: &Path) -> Option<String> {
    let root = git_root().ok()?;
    std::fs::read_to_string(root.join(path)).ok()
}

/// Maps file extensions to language names.
pub fn language_from_ext(path: &Path) -> String {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => "Rust",
        Some("lua") => "Lua",
        Some("toml") => "TOML",
        Some("json") => "JSON",
        Some("js" | "mjs" | "cjs") => "JavaScript",
        Some("ts" | "mts" | "cts") => "TypeScript",
        Some("py") => "Python",
        Some("go") => "Go",
        Some("c" | "h") => "C",
        Some("cpp" | "cc" | "cxx" | "hpp") => "C++",
        Some("java") => "Java",
        Some("rb") => "Ruby",
        Some("sh" | "bash" | "zsh") => "Shell",
        Some("md") => "Markdown",
        Some("yml" | "yaml") => "YAML",
        Some("html" | "htm") => "HTML",
        Some("css") => "CSS",
        _ => "Text",
    }
    .to_string()
}

/// Splits content into lines.
pub fn into_lines(content: Option<String>) -> Vec<String> {
    content
        .map(|c| c.lines().map(String::from).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_git_range_single_commit() {
        let (old, new) = parse_git_range("abc123");
        assert_eq!(old, "abc123^");
        assert_eq!(new, "abc123");
    }

    #[test]
    fn test_parse_git_range_double_dot() {
        let (old, new) = parse_git_range("main..feature");
        assert_eq!(old, "main");
        assert_eq!(new, "feature");
    }

    #[test]
    fn test_parse_name_status() {
        let entries = parse_name_status_entries("M\tsrc/lib.rs\nA\tsrc/new.rs\nD\tsrc/old.rs\n");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].status, "M");
        assert_eq!(entries[1].status, "A");
        assert_eq!(entries[2].status, "D");
    }

    #[test]
    fn test_parse_name_status_rename() {
        let entries = parse_name_status_entries("R100\tsrc/old.rs\tsrc/new.rs\n");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].status, "R100");
        assert_eq!(entries[0].old_path, PathBuf::from("src/old.rs"));
        assert_eq!(entries[0].new_path, PathBuf::from("src/new.rs"));
    }

    #[test]
    fn test_into_lines() {
        let lines = into_lines(Some("a\nb\nc".to_string()));
        assert_eq!(lines, vec!["a", "b", "c"]);
        assert!(into_lines(None).is_empty());
    }
}
