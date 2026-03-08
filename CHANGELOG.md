# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-03-08

### Added

- Commit log view with search filtering
- Side-by-side diff view with syntax-aware character-level highlights from difftastic
- Collapsible file tree sidebar with change stats
- Hunk navigation within and across files
- Compare flow for diffing any two endpoints (commits, staged, unstaged, working tree)
- Color-coded scrollbar with change markers
- Help overlay (`?`)
- CLI flags: `--staged`, `--unstaged`, `--no-tree`, `--tree-width`
- Range support: `main..feature`, `main...feature`
