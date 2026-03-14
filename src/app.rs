//! Application state machine, event loop, and diff loading logic.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{self, Event};
use ratatui::DefaultTerminal;

use crate::difft;
use crate::git::{self, LogEntry};
use crate::input::{Action, InputState, ViewContext};
use crate::integrity;
use crate::nav::{HunkJump, NavState};
use crate::processor;
use crate::review;
use crate::types::{DisplayFile, FileStatus};
use crate::ui;
use crate::ui::tree_pane::{self, TreeNode};

/// What the application is currently showing.
pub enum View {
    Log,
    Diff,
    Compare(CompareState),
    Loading(String),
    Error(String),
    Help,
}

/// Compare flow state.
pub enum CompareState {
    PickNew {
        items: Vec<CompareItem>,
        cursor: usize,
        scroll: usize,
    },
    PickOld {
        new_rev: String,
        new_label: String,
        items: Vec<CompareItem>,
        cursor: usize,
        scroll: usize,
    },
}

#[derive(Debug, Clone)]
pub struct CompareItem {
    pub rev: String,
    pub label: String,
    pub is_special: bool,
    pub short_hash: Option<String>,
    pub date: Option<String>,
    pub subject: Option<String>,
}

/// An item in the log view: either a special entry or a commit.
#[derive(Debug, Clone)]
pub enum LogItem {
    WorkingTree,
    Staged,
    Separator,
    Commit(LogEntry),
}

/// A match found by the in-diff search.
#[derive(Debug, Clone)]
pub struct DiffFindMatch {
    pub row: usize,
    pub col: usize,
    pub len: usize,
    /// true = left (old) side, false = right (new) side
    pub is_left: bool,
}

/// Message sent from the background diff thread.
pub enum DiffMessage {
    Done(DiffPayload),
    Error(String),
}

/// Result of a background diff computation.
pub struct DiffPayload {
    pub files: Vec<DisplayFile>,
}

/// State tracked while a diff is loading in the background.
pub struct LoadingState {
    pub rx: mpsc::Receiver<DiffMessage>,
    pub completed: Arc<AtomicUsize>,
    pub tick: usize,
}

/// Main application state.
pub struct App {
    pub view: View,
    /// Whether the app was launched with args (no log view to go back to).
    pub launched_with_args: bool,

    // Diff view state
    pub files: Vec<DisplayFile>,
    pub nav: NavState,
    pub show_tree: bool,
    pub tree_width: u16,
    pub tree_nodes: Vec<TreeNode>,
    pub tree_scroll: usize,
    pub tree_focused: bool,
    pub auto_hide_tree: bool,
    pub tree_cursor: usize,
    pub diff_context: Option<String>,
    /// Current cursor row in diff view (highlighted line).
    pub diff_cursor: usize,

    // Log view state
    pub log_items: Vec<LogItem>,
    pub log_entries: Vec<LogEntry>,
    pub log_cursor: usize,
    pub log_scroll: usize,

    // Search state (shared between log and compare views)
    pub search_active: bool,
    pub search_query: String,
    /// Indices into the current list that match the search query.
    pub search_filtered: Vec<usize>,
    /// Cursor position within the filtered list.
    pub search_cursor: usize,
    /// Scroll offset within the filtered list.
    pub search_scroll: usize,

    // Diff find state
    pub diff_find_active: bool,
    pub diff_find_query: String,
    pub diff_find_search_old: bool,
    pub diff_find_search_new: bool,
    pub diff_find_matches: Vec<DiffFindMatch>,
    pub diff_find_current: usize,

    // Input
    pub input_state: InputState,

    // Background loading
    pub loading_state: Option<LoadingState>,

    // Review tracking
    pub reviewed: HashSet<usize>,
    pub review_store: Option<review::ReviewStore>,
    pub diff_scope: Option<String>,

    // For diff refresh
    pub last_diff_mode: Option<DiffMode>,

    // Refresh status (displayed at bottom of file tree)
    pub diff_loaded_time: Option<String>,
    pub initial_file_count: usize,
    pub diff_refreshed_time: Option<String>,
    pub refresh_delta_text: String,
    pub baseline_file_paths: HashSet<PathBuf>,
    pub baseline_file_hashes: HashMap<PathBuf, u64>,
}

impl App {
    pub fn new() -> Self {
        Self {
            view: View::Log,
            launched_with_args: false,
            files: Vec::new(),
            nav: NavState::new(0),
            show_tree: true,
            tree_width: 35,
            tree_nodes: Vec::new(),
            tree_scroll: 0,
            tree_focused: false,
            auto_hide_tree: false,
            tree_cursor: 0,
            diff_context: None,
            diff_cursor: 0,
            log_items: Vec::new(),
            log_entries: Vec::new(),
            log_cursor: 0,
            log_scroll: 0,
            search_active: false,
            search_query: String::new(),
            search_filtered: Vec::new(),
            search_cursor: 0,
            search_scroll: 0,
            diff_find_active: false,
            diff_find_query: String::new(),
            diff_find_search_old: false,
            diff_find_search_new: true,
            diff_find_matches: Vec::new(),
            diff_find_current: 0,
            input_state: InputState::default(),
            loading_state: None,
            reviewed: HashSet::new(),
            review_store: git::git_root().ok().and_then(|r| review::ReviewStore::open(&r)),
            diff_scope: None,
            last_diff_mode: None,
            diff_loaded_time: None,
            initial_file_count: 0,
            diff_refreshed_time: None,
            refresh_delta_text: String::new(),
            baseline_file_paths: HashSet::new(),
            baseline_file_hashes: HashMap::new(),
        }
    }

    /// Spawn a background thread to compute the diff.
    pub fn start_diff_loading(&mut self, mode: DiffMode, context: Option<String>) {
        let (tx, rx) = mpsc::channel();
        let completed = Arc::new(AtomicUsize::new(0));
        let completed_clone = completed.clone();

        self.last_diff_mode = Some(mode.clone());
        self.diff_scope = Some(mode.scope_key());
        self.diff_context = context.clone();
        let loading_msg = context.unwrap_or_else(|| "Computing diff...".to_string());
        self.view = View::Loading(loading_msg);
        self.loading_state = Some(LoadingState { rx, completed, tick: 0 });

        std::thread::spawn(move || {
            let result = run_diff_background(mode, completed_clone);
            let _ = tx.send(match result {
                Ok(files) => DiffMessage::Done(DiffPayload { files }),
                Err(e) => DiffMessage::Error(e),
            });
        });
    }

    /// Apply a completed diff result to the app state.
    fn apply_diff_result(&mut self, payload: DiffPayload) {
        let is_refresh = self.diff_loaded_time.is_some();

        if is_refresh {
            // Compute delta before replacing files
            self.refresh_delta_text = self.compute_refresh_delta(&payload.files);
            self.diff_refreshed_time = Some(Self::fmt_now());
        }

        self.files = payload.files;
        self.nav = NavState::new(self.files.len());
        self.tree_nodes = tree_pane::build_tree(&self.files);
        self.tree_scroll = 0;
        self.tree_cursor = 0;
        self.nav.auto_scroll_to_first_hunk(&self.files);
        self.diff_cursor = self.nav.scroll();

        // Load review marks
        self.reviewed = if let (Some(store), Some(scope)) = (&self.review_store, &self.diff_scope) {
            store.reviewed_set(scope, &self.files)
        } else {
            HashSet::new()
        };

        if !is_refresh {
            self.diff_loaded_time = Some(Self::fmt_now());
            self.initial_file_count = self.files.len();
            self.diff_refreshed_time = None;
            self.refresh_delta_text.clear();
        }

        // Capture baseline for next refresh delta
        self.baseline_file_paths = self.files.iter().map(|f| f.path.clone()).collect();
        self.baseline_file_hashes = self.files.iter().map(|f| (f.path.clone(), f.content_hash)).collect();

        self.view = View::Diff;
    }

    /// Compute a delta string comparing new files against the current baseline.
    fn compute_refresh_delta(&self, new_files: &[DisplayFile]) -> String {
        if self.baseline_file_paths.is_empty() && !new_files.is_empty() {
            return "? missing baseline".to_string();
        }

        let current_paths: HashSet<PathBuf> = new_files.iter().map(|f| f.path.clone()).collect();
        let mut added = 0usize;
        let mut removed = 0usize;
        let mut changed = 0usize;

        for f in new_files {
            if !self.baseline_file_paths.contains(&f.path) {
                added += 1;
            } else if let Some(&old_hash) = self.baseline_file_hashes.get(&f.path) {
                if old_hash != 0 && f.content_hash != 0 && old_hash != f.content_hash {
                    changed += 1;
                }
            }
        }
        for p in &self.baseline_file_paths {
            if !current_paths.contains(p) {
                removed += 1;
            }
        }

        let mut parts = Vec::new();
        if added > 0 { parts.push(format!("+{added} new")); }
        if removed > 0 { parts.push(format!("-{removed} removed")); }
        if changed > 0 { parts.push(format!("{changed} changed")); }
        if parts.is_empty() { return "no changes".to_string(); }
        parts.join(" \u{00b7} ")
    }

    /// Format the current local time as HH:MM:SS.
    fn fmt_now() -> String {
        let now: std::time::SystemTime = std::time::SystemTime::now();
        let secs = now
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Apply local UTC offset (use libc localtime)
        let local_secs = {
            let t = secs as libc::time_t;
            let mut tm: libc::tm = unsafe { std::mem::zeroed() };
            unsafe { libc::localtime_r(&t, &mut tm) };
            ((tm.tm_hour * 3600 + tm.tm_min * 60 + tm.tm_sec) as u64) % 86400
        };
        let h = local_secs / 3600;
        let m = (local_secs % 3600) / 60;
        let s = local_secs % 60;
        format!("{h:02}:{m:02}:{s:02}")
    }

    /// Cancel a loading operation and return to the previous view.
    /// Returns true if the app should exit.
    fn cancel_loading(&mut self) -> bool {
        self.loading_state = None;
        if self.launched_with_args {
            return true;
        }
        self.view = View::Log;
        false
    }

    /// Load the commit log.
    pub fn load_log(&mut self) -> Result<(), String> {
        self.log_entries = git::git_log(200)?;
        self.rebuild_log_items();
        self.log_cursor = 0;
        self.log_scroll = 0;
        self.view = View::Log;
        Ok(())
    }

    /// Rebuild the log_items list from current state.
    pub fn rebuild_log_items(&mut self) {
        let mut items = Vec::new();
        let mut has_special = false;

        if git::has_unstaged_changes() {
            items.push(LogItem::WorkingTree);
            has_special = true;
        }
        if git::has_staged_changes() {
            items.push(LogItem::Staged);
            has_special = true;
        }
        if has_special {
            items.push(LogItem::Separator);
        }

        for entry in &self.log_entries {
            items.push(LogItem::Commit(entry.clone()));
        }

        self.log_items = items;
    }

    /// Run the main event loop.
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> std::io::Result<()> {
        loop {
            terminal.draw(|f| ui::draw(f, self))?;

            // Check for background diff results
            if self.loading_state.is_some() {
                self.loading_state.as_mut().unwrap().tick += 1;

                match self.loading_state.as_ref().unwrap().rx.try_recv() {
                    Ok(DiffMessage::Done(payload)) => {
                        self.loading_state = None;
                        self.apply_diff_result(payload);
                        continue; // Redraw immediately with new view
                    }
                    Ok(DiffMessage::Error(e)) => {
                        self.loading_state = None;
                        self.view = View::Error(e);
                        continue;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.loading_state = None;
                        self.view = View::Error(
                            "Diff computation failed unexpectedly".to_string(),
                        );
                        continue;
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }

            // Wait for input: poll during loading (for spinner), block otherwise
            let has_event = if self.loading_state.is_some() {
                event::poll(Duration::from_millis(66))?
            } else {
                true // event::read() below will block
            };

            if !has_event {
                continue;
            }

            if let Event::Key(key) = event::read()? {
                // Diff find mode intercepts all key events
                if self.diff_find_active {
                    self.handle_diff_find_key(key);
                    continue;
                }

                // Search mode intercepts all key events
                if self.search_active {
                    self.handle_search_key(key);
                    continue;
                }

                let view_context = match &self.view {
                    View::Log => ViewContext::Log,
                    View::Diff if self.tree_focused => ViewContext::Tree,
                    View::Diff => ViewContext::Diff,
                    View::Compare(_) => ViewContext::Compare,
                    View::Help => ViewContext::Diff,
                    View::Loading(_) | View::Error(_) => ViewContext::Diff,
                };

                let action = self.input_state.handle_key(key, view_context);

                match &self.view {
                    View::Help => {
                        if matches!(
                            action,
                            Action::Quit | Action::ShowHelp
                        ) {
                            self.view = View::Diff;
                        }
                        continue;
                    }
                    View::Error(_) => {
                        if matches!(action, Action::Quit) {
                            if self.launched_with_args {
                                return Ok(());
                            }
                            self.view = View::Log;
                        }
                        continue;
                    }
                    View::Loading(_) => {
                        if matches!(action, Action::Quit) {
                            if self.cancel_loading() {
                                return Ok(());
                            }
                        }
                        continue;
                    }
                    _ => {}
                }

                if self.handle_action(action)? {
                    return Ok(());
                }
            }
        }
    }

    /// Handle an action. Returns true if the app should quit.
    fn handle_action(&mut self, action: Action) -> std::io::Result<bool> {
        match &self.view {
            View::Log => return Ok(self.handle_log_action(action)),
            View::Diff => return Ok(self.handle_diff_action(action)),
            View::Compare(_) => return Ok(self.handle_compare_action(action)),
            _ => {}
        }
        Ok(false)
    }

    fn handle_log_action(&mut self, action: Action) -> bool {
        let item_count = self.log_items.len();
        match action {
            Action::Quit => return true,
            Action::ScrollDown(n) => {
                self.log_cursor = (self.log_cursor + n).min(item_count.saturating_sub(1));
                self.skip_separator_down(item_count);
                self.ensure_log_visible();
            }
            Action::ScrollUp(n) => {
                self.log_cursor = self.log_cursor.saturating_sub(n);
                self.skip_separator_up();
                self.ensure_log_visible();
            }
            Action::ScrollToTop => {
                self.log_cursor = 0;
                self.log_scroll = 0;
            }
            Action::ScrollToBottom => {
                self.log_cursor = item_count.saturating_sub(1);
                self.ensure_log_visible();
            }
            Action::HalfPageDown => {
                self.log_cursor = (self.log_cursor + 15).min(item_count.saturating_sub(1));
                self.skip_separator_down(item_count);
                self.ensure_log_visible();
            }
            Action::HalfPageUp => {
                self.log_cursor = self.log_cursor.saturating_sub(15);
                self.skip_separator_up();
                self.ensure_log_visible();
            }
            Action::Select => {
                if let Some(item) = self.log_items.get(self.log_cursor).cloned() {
                    match item {
                        LogItem::WorkingTree => {
                            self.start_diff_loading(DiffMode::Unstaged, Some("Working Tree".to_string()));
                        }
                        LogItem::Staged => {
                            self.start_diff_loading(DiffMode::Staged, Some("Staged".to_string()));
                        }
                        LogItem::Commit(entry) => {
                            let range = entry.full_hash.clone();
                            let date_only = entry.date.split(' ').next().unwrap_or(&entry.date);
                            let context = format!("{} {} {}", entry.short_hash, date_only, truncate(&entry.subject, 20));
                            self.start_diff_loading(DiffMode::Range(range), Some(context));
                        }
                        LogItem::Separator => {}
                    }
                }
            }
            Action::StartCompare => {
                self.start_compare();
            }
            Action::StartSearch => {
                self.activate_search();
            }
            Action::ShowHelp => {
                self.view = View::Help;
            }
            _ => {}
        }
        false
    }

    /// Skip over separator lines when scrolling down.
    fn skip_separator_down(&mut self, item_count: usize) {
        if matches!(self.log_items.get(self.log_cursor), Some(LogItem::Separator)) {
            if self.log_cursor + 1 < item_count {
                self.log_cursor += 1;
            }
        }
    }

    /// Skip over separator lines when scrolling up.
    fn skip_separator_up(&mut self) {
        if matches!(self.log_items.get(self.log_cursor), Some(LogItem::Separator)) {
            if self.log_cursor > 0 {
                self.log_cursor -= 1;
            }
        }
    }

    fn handle_diff_action(&mut self, action: Action) -> bool {
        let viewport_height = 30; // approximate

        match action {
            Action::Quit => {
                self.reviewed.clear();
                self.diff_scope = None;
                self.last_diff_mode = None;
                self.diff_loaded_time = None;
                self.initial_file_count = 0;
                self.diff_refreshed_time = None;
                self.refresh_delta_text.clear();
                self.baseline_file_paths.clear();
                self.baseline_file_hashes.clear();
                if self.launched_with_args {
                    return true;
                }
                self.view = View::Log;
            }
            Action::ScrollDown(n) => {
                if self.tree_focused {
                    let flat = tree_pane::flatten_visible(&self.tree_nodes, 0);
                    self.tree_cursor = (self.tree_cursor + n).min(flat.len().saturating_sub(1));
                    self.ensure_tree_visible(flat.len());
                } else {
                    let max = self.current_file_rows().saturating_sub(1);
                    self.diff_cursor = (self.diff_cursor + n).min(max);
                    self.ensure_diff_cursor_visible(viewport_height);
                }
            }
            Action::ScrollUp(n) => {
                if self.tree_focused {
                    self.tree_cursor = self.tree_cursor.saturating_sub(n);
                    self.ensure_tree_visible(tree_pane::flatten_visible(&self.tree_nodes, 0).len());
                } else {
                    self.diff_cursor = self.diff_cursor.saturating_sub(n);
                    self.ensure_diff_cursor_visible(viewport_height);
                }
            }
            Action::ScrollToTop => {
                if self.tree_focused {
                    self.tree_cursor = 0;
                    self.tree_scroll = 0;
                } else {
                    self.diff_cursor = 0;
                    self.nav.set_scroll(0);
                }
            }
            Action::ScrollToBottom => {
                if self.tree_focused {
                    let flat = tree_pane::flatten_visible(&self.tree_nodes, 0);
                    self.tree_cursor = flat.len().saturating_sub(1);
                    self.ensure_tree_visible(flat.len());
                } else {
                    let max = self.current_file_rows().saturating_sub(1);
                    self.diff_cursor = max;
                    self.ensure_diff_cursor_visible(viewport_height);
                }
            }
            Action::HalfPageDown => {
                let half = viewport_height / 2;
                if self.tree_focused {
                    let flat = tree_pane::flatten_visible(&self.tree_nodes, 0);
                    self.tree_cursor = (self.tree_cursor + half).min(flat.len().saturating_sub(1));
                    self.ensure_tree_visible(flat.len());
                } else {
                    let max = self.current_file_rows().saturating_sub(1);
                    self.diff_cursor = (self.diff_cursor + half).min(max);
                    self.ensure_diff_cursor_visible(viewport_height);
                }
            }
            Action::HalfPageUp => {
                let half = viewport_height / 2;
                if self.tree_focused {
                    self.tree_cursor = self.tree_cursor.saturating_sub(half);
                    self.ensure_tree_visible(tree_pane::flatten_visible(&self.tree_nodes, 0).len());
                } else {
                    self.diff_cursor = self.diff_cursor.saturating_sub(half);
                    self.ensure_diff_cursor_visible(viewport_height);
                }
            }
            Action::ScrollLeft(n) => {
                let new_h = self.nav.h_scroll().saturating_sub(n);
                self.nav.set_h_scroll(new_h);
            }
            Action::ScrollRight(n) => {
                let new_h = self.nav.h_scroll() + n;
                self.nav.set_h_scroll(new_h);
            }
            Action::ScrollToLineStart => {
                self.nav.set_h_scroll(0);
            }
            Action::ScrollToLineEnd => {
                let max_len = self.current_file_max_line_len();
                self.nav.set_h_scroll(max_len.saturating_sub(40));
            }
            Action::NextHunk => {
                if let Some(jump) = self.nav.next_hunk(&self.files) {
                    match jump {
                        HunkJump::SameFile(row) => {
                            self.diff_cursor = row;
                            self.nav.set_scroll(row);
                        }
                        HunkJump::NextFile(idx, row) => {
                            self.nav.go_to_file(idx, self.files.len());
                            self.diff_cursor = row;
                            self.nav.set_scroll(row);
                        }
                    }
                }
            }
            Action::PrevHunk => {
                if let Some(jump) = self.nav.prev_hunk(&self.files) {
                    match jump {
                        HunkJump::SameFile(row) => {
                            self.diff_cursor = row;
                            self.nav.set_scroll(row);
                        }
                        HunkJump::NextFile(idx, row) => {
                            self.nav.go_to_file(idx, self.files.len());
                            self.diff_cursor = row;
                            self.nav.set_scroll(row);
                        }
                    }
                }
            }
            Action::NextFile => {
                if self.nav.next_file(self.files.len()) {
                    self.nav.auto_scroll_to_first_hunk(&self.files);
                    self.diff_cursor = self.nav.scroll();
                    self.diff_find_matches.clear();
                    self.diff_find_current = 0;
                }
            }
            Action::PrevFile => {
                if self.nav.prev_file() {
                    self.nav.auto_scroll_to_first_hunk(&self.files);
                    self.diff_cursor = self.nav.scroll();
                    self.diff_find_matches.clear();
                    self.diff_find_current = 0;
                }
            }
            Action::ToggleTreeFocus => {
                self.tree_focused = !self.tree_focused;
                if self.tree_focused && !self.show_tree {
                    self.show_tree = true;
                }
                if !self.tree_focused && self.auto_hide_tree {
                    self.show_tree = false;
                }
            }
            Action::ToggleTree => {
                self.show_tree = !self.show_tree;
                self.tree_focused = self.show_tree;
            }
            Action::Select => {
                if self.tree_focused {
                    let flat = tree_pane::flatten_visible(&self.tree_nodes, 0);
                    if let Some(node) = flat.get(self.tree_cursor) {
                        if let Some(file_idx) = node.file_idx {
                            self.nav.go_to_file(file_idx, self.files.len());
                            self.nav.auto_scroll_to_first_hunk(&self.files);
                            self.diff_cursor = self.nav.scroll();
                            self.tree_focused = false;
                            if self.auto_hide_tree {
                                self.show_tree = false;
                            }
                        } else if node.is_dir {
                            tree_pane::toggle_node(&mut self.tree_nodes, self.tree_cursor);
                        }
                    }
                }
            }
            Action::ToggleCollapse => {
                if self.tree_focused {
                    tree_pane::toggle_node(&mut self.tree_nodes, self.tree_cursor);
                }
            }
            Action::ToggleReviewed => {
                let file_idx = if self.tree_focused {
                    let flat = tree_pane::flatten_visible(&self.tree_nodes, 0);
                    flat.get(self.tree_cursor).and_then(|n| n.file_idx)
                } else {
                    Some(self.nav.current_file)
                };

                if let Some(idx) = file_idx {
                    if let Some(file) = self.files.get(idx) {
                        let path = file.path.to_string_lossy().to_string();
                        let hash = file.content_hash;

                        if self.reviewed.contains(&idx) {
                            self.reviewed.remove(&idx);
                            if let (Some(store), Some(scope)) = (&self.review_store, &self.diff_scope) {
                                store.unmark(scope, &path);
                            }
                        } else {
                            self.reviewed.insert(idx);
                            if let (Some(store), Some(scope)) = (&self.review_store, &self.diff_scope) {
                                store.mark(scope, &path, hash);
                            }
                        }
                    }
                }
            }
            Action::ClearAllReviews => {
                self.reviewed.clear();
                if let Some(store) = &self.review_store {
                    store.clear_all();
                }
            }
            Action::StartDiffFind => {
                self.diff_find_active = true;
                self.diff_find_query.clear();
                self.diff_find_current = 0;
                self.diff_find_matches.clear();
            }
            Action::RefreshDiff => {
                if let Some(mode) = self.last_diff_mode.clone() {
                    let context = self.diff_context.clone();
                    self.start_diff_loading(mode, context);
                }
            }
            Action::ShowHelp => {
                self.view = View::Help;
            }
            _ => {}
        }
        false
    }

    fn ensure_diff_cursor_visible(&mut self, viewport_height: usize) {
        let scroll = self.nav.scroll();
        if self.diff_cursor < scroll {
            self.nav.set_scroll(self.diff_cursor);
        } else if self.diff_cursor >= scroll + viewport_height {
            self.nav.set_scroll(self.diff_cursor.saturating_sub(viewport_height - 1));
        }
    }

    fn handle_compare_action(&mut self, action: Action) -> bool {
        let view = std::mem::replace(&mut self.view, View::Log);
        let View::Compare(compare) = view else {
            self.view = view;
            return false;
        };

        match action {
            Action::Quit => {
                self.view = View::Log;
                return false;
            }
            Action::ScrollDown(n) => {
                let (items, cursor, scroll) = match compare {
                    CompareState::PickNew {
                        items,
                        cursor,
                        scroll,
                    } => (items, cursor, scroll),
                    CompareState::PickOld {
                        items,
                        cursor,
                        scroll,
                        new_rev,
                        new_label,
                    } => {
                        let new_cursor = (cursor + n).min(items.len().saturating_sub(1));
                        self.view = View::Compare(CompareState::PickOld {
                            items,
                            cursor: new_cursor,
                            scroll: adjust_scroll(scroll, new_cursor, 30),
                            new_rev,
                            new_label,
                        });
                        return false;
                    }
                };
                let new_cursor = (cursor + n).min(items.len().saturating_sub(1));
                self.view = View::Compare(CompareState::PickNew {
                    items,
                    cursor: new_cursor,
                    scroll: adjust_scroll(scroll, new_cursor, 30),
                });
            }
            Action::ScrollUp(n) => {
                let (items, cursor, scroll) = match compare {
                    CompareState::PickNew {
                        items,
                        cursor,
                        scroll,
                    } => (items, cursor, scroll),
                    CompareState::PickOld {
                        items,
                        cursor,
                        scroll,
                        new_rev,
                        new_label,
                    } => {
                        let new_cursor = cursor.saturating_sub(n);
                        self.view = View::Compare(CompareState::PickOld {
                            items,
                            cursor: new_cursor,
                            scroll: adjust_scroll(scroll, new_cursor, 30),
                            new_rev,
                            new_label,
                        });
                        return false;
                    }
                };
                let new_cursor = cursor.saturating_sub(n);
                self.view = View::Compare(CompareState::PickNew {
                    items,
                    cursor: new_cursor,
                    scroll: adjust_scroll(scroll, new_cursor, 30),
                });
            }
            Action::ScrollToTop => {
                match compare {
                    CompareState::PickNew { items, .. } => {
                        self.view = View::Compare(CompareState::PickNew {
                            items,
                            cursor: 0,
                            scroll: 0,
                        });
                    }
                    CompareState::PickOld {
                        items,
                        new_rev,
                        new_label,
                        ..
                    } => {
                        self.view = View::Compare(CompareState::PickOld {
                            items,
                            cursor: 0,
                            scroll: 0,
                            new_rev,
                            new_label,
                        });
                    }
                }
            }
            Action::ScrollToBottom => {
                match compare {
                    CompareState::PickNew { items, .. } => {
                        let last = items.len().saturating_sub(1);
                        self.view = View::Compare(CompareState::PickNew {
                            cursor: last,
                            scroll: last.saturating_sub(29),
                            items,
                        });
                    }
                    CompareState::PickOld {
                        items,
                        new_rev,
                        new_label,
                        ..
                    } => {
                        let last = items.len().saturating_sub(1);
                        self.view = View::Compare(CompareState::PickOld {
                            cursor: last,
                            scroll: last.saturating_sub(29),
                            items,
                            new_rev,
                            new_label,
                        });
                    }
                }
            }
            Action::Select => {
                match compare {
                    CompareState::PickNew { items, cursor, .. } => {
                        if let Some(item) = items.get(cursor) {
                            let new_rev = item.rev.clone();
                            let new_label = item.label.clone();

                            // Build old-side items
                            let old_items = build_old_items(&new_rev);
                            self.view = View::Compare(CompareState::PickOld {
                                new_rev,
                                new_label,
                                items: old_items,
                                cursor: 0,
                                scroll: 0,
                            });
                        }
                    }
                    CompareState::PickOld {
                        new_rev,
                        new_label,
                        items,
                        cursor,
                        ..
                    } => {
                        if let Some(item) = items.get(cursor) {
                            let old_rev = item.rev.clone();
                            let old_short = short_context(item);
                            let new_short = short_context_from_label(&new_label);
                            let context = format!("{} -> {}", old_short, new_short);

                            let mode = resolve_compare_mode(&old_rev, &new_rev);
                            self.start_diff_loading(mode, Some(context));
                        }
                    }
                }
            }
            Action::StartSearch => {
                self.view = View::Compare(compare);
                self.activate_search();
            }
            _ => {
                self.view = View::Compare(compare);
            }
        }
        false
    }

    fn start_compare(&mut self) {
        let mut items: Vec<CompareItem> = Vec::new();

        // Special endpoints
        if git::has_unstaged_changes() {
            items.push(CompareItem {
                rev: "--working-tree".to_string(),
                label: "WORKING TREE".to_string(),
                is_special: true,
                short_hash: None,
                date: None,
                subject: None,
            });
        }

        if git::has_staged_changes() {
            items.push(CompareItem {
                rev: "--staged".to_string(),
                label: "STAGED (INDEX)".to_string(),
                is_special: true,
                short_hash: None,
                date: None,
                subject: None,
            });
        }

        // Commit entries
        for entry in &self.log_entries {
            items.push(CompareItem {
                rev: entry.full_hash.clone(),
                label: format!("{} {} {}", entry.short_hash, entry.date, truncate(&entry.subject, 30)),
                is_special: false,
                short_hash: Some(entry.short_hash.clone()),
                date: Some(entry.date.clone()),
                subject: Some(entry.subject.clone()),
            });
        }

        self.view = View::Compare(CompareState::PickNew {
            items,
            cursor: 0,
            scroll: 0,
        });
    }

    fn ensure_log_visible(&mut self) {
        let viewport = 30; // approximate
        if self.log_cursor < self.log_scroll {
            self.log_scroll = self.log_cursor;
        } else if self.log_cursor >= self.log_scroll + viewport {
            self.log_scroll = self.log_cursor.saturating_sub(viewport - 1);
        }
    }

    fn ensure_tree_visible(&mut self, _total: usize) {
        let viewport = 30; // approximate
        if self.tree_cursor < self.tree_scroll {
            self.tree_scroll = self.tree_cursor;
        } else if self.tree_cursor >= self.tree_scroll + viewport {
            self.tree_scroll = self.tree_cursor.saturating_sub(viewport - 1);
        }
    }

    fn current_file_rows(&self) -> usize {
        self.files
            .get(self.nav.current_file)
            .map(|f| f.rows.len())
            .unwrap_or(0)
    }

    fn current_file_max_line_len(&self) -> usize {
        self.files
            .get(self.nav.current_file)
            .map(|f| {
                f.rows
                    .iter()
                    .map(|r| r.left.content.len().max(r.right.content.len()))
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0)
    }

    fn activate_search(&mut self) {
        self.search_active = true;
        self.search_query.clear();
        self.search_cursor = 0;
        self.search_scroll = 0;
        self.update_search_filter();
    }

    fn deactivate_search(&mut self) {
        self.search_active = false;
        self.search_query.clear();
        self.search_filtered.clear();
        self.search_cursor = 0;
        self.search_scroll = 0;
    }

    fn handle_search_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::{KeyCode, KeyModifiers};

        let modifiers = key.modifiers;

        match (key.code, modifiers) {
            (KeyCode::Esc, _) => {
                self.deactivate_search();
            }
            (KeyCode::Enter, _) => {
                self.search_confirm();
            }
            (KeyCode::Backspace, _) => {
                self.search_query.pop();
                self.update_search_filter();
            }
            (KeyCode::Down, KeyModifiers::CONTROL) => {
                let max = self.search_filtered.len().saturating_sub(1);
                self.search_move_cursor_to(max);
            }
            (KeyCode::Up, KeyModifiers::CONTROL) => {
                self.search_move_cursor_to(0);
            }
            (KeyCode::Down, KeyModifiers::SHIFT) => {
                self.search_move_cursor(5);
            }
            (KeyCode::Up, KeyModifiers::SHIFT) => {
                self.search_move_cursor(-5);
            }
            (KeyCode::Down, KeyModifiers::NONE) => {
                self.search_move_cursor(1);
            }
            (KeyCode::Up, KeyModifiers::NONE) => {
                self.search_move_cursor(-1);
            }
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                self.search_query.push(c);
                self.update_search_filter();
            }
            _ => {}
        }
    }

    fn update_search_filter(&mut self) {
        let query = self.search_query.to_lowercase();

        match &self.view {
            View::Log => {
                if query.is_empty() {
                    self.search_filtered = (0..self.log_items.len())
                        .filter(|i| !matches!(self.log_items.get(*i), Some(LogItem::Separator)))
                        .collect();
                } else {
                    self.search_filtered = self
                        .log_items
                        .iter()
                        .enumerate()
                        .filter(|(_, item)| match item {
                            LogItem::WorkingTree => "working tree".contains(&query),
                            LogItem::Staged => "staged index".contains(&query),
                            LogItem::Separator => false,
                            LogItem::Commit(e) => {
                                e.short_hash.to_lowercase().contains(&query)
                                    || e.full_hash.to_lowercase().contains(&query)
                                    || e.date.to_lowercase().contains(&query)
                                    || e.subject.to_lowercase().contains(&query)
                            }
                        })
                        .map(|(i, _)| i)
                        .collect();
                }
            }
            View::Compare(compare) => {
                let items = match compare {
                    CompareState::PickNew { items, .. } => items,
                    CompareState::PickOld { items, .. } => items,
                };
                if query.is_empty() {
                    self.search_filtered = (0..items.len()).collect();
                } else {
                    self.search_filtered = items
                        .iter()
                        .enumerate()
                        .filter(|(_, item)| item.label.to_lowercase().contains(&query))
                        .map(|(i, _)| i)
                        .collect();
                }
            }
            _ => {}
        }

        // Reset cursor to first result
        self.search_cursor = 0;
        self.search_scroll = 0;
        // Update the real cursor to point to the first filtered item
        self.sync_search_cursor_to_real();
    }

    fn search_move_cursor_to(&mut self, pos: usize) {
        if self.search_filtered.is_empty() {
            return;
        }
        self.search_cursor = pos.min(self.search_filtered.len() - 1);

        let viewport = 30usize;
        if self.search_cursor < self.search_scroll {
            self.search_scroll = self.search_cursor;
        } else if self.search_cursor >= self.search_scroll + viewport {
            self.search_scroll = self.search_cursor.saturating_sub(viewport - 1);
        }

        self.sync_search_cursor_to_real();
    }

    fn search_move_cursor(&mut self, delta: i32) {
        if self.search_filtered.is_empty() {
            return;
        }

        let max = self.search_filtered.len() - 1;
        if delta > 0 {
            self.search_cursor = (self.search_cursor + delta as usize).min(max);
        } else {
            self.search_cursor = self.search_cursor.saturating_sub((-delta) as usize);
        }

        // Keep cursor visible within the search scroll viewport
        let viewport = 30usize; // approximate
        if self.search_cursor < self.search_scroll {
            self.search_scroll = self.search_cursor;
        } else if self.search_cursor >= self.search_scroll + viewport {
            self.search_scroll = self.search_cursor.saturating_sub(viewport - 1);
        }

        self.sync_search_cursor_to_real();
    }

    /// Sync the search_cursor position to the actual log/compare cursor.
    fn sync_search_cursor_to_real(&mut self) {
        let real_idx = self
            .search_filtered
            .get(self.search_cursor)
            .copied()
            .unwrap_or(0);

        match &self.view {
            View::Log => {
                self.log_cursor = real_idx;
            }
            View::Compare(_) => {
                self.set_compare_cursor(real_idx);
            }
            _ => {}
        }
    }

    fn search_confirm(&mut self) {
        // Close search, then trigger Select on whatever view we're in
        self.search_active = false;
        self.search_query.clear();
        self.search_filtered.clear();
        self.search_cursor = 0;
        self.search_scroll = 0;

        // Trigger the same action as pressing Enter
        self.handle_action(Action::Select).ok();
    }

    fn handle_diff_find_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::{KeyCode, KeyModifiers};

        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) | (KeyCode::Char('f'), KeyModifiers::CONTROL) => {
                self.diff_find_active = false;
                self.diff_find_query.clear();
                self.diff_find_matches.clear();
                self.diff_find_current = 0;
            }
            (KeyCode::Enter, KeyModifiers::SHIFT) => {
                self.prev_diff_find_match();
            }
            (KeyCode::Enter, KeyModifiers::NONE) | (KeyCode::Down, KeyModifiers::NONE) => {
                self.next_diff_find_match();
            }
            (KeyCode::Up, KeyModifiers::NONE) => {
                self.prev_diff_find_match();
            }
            (KeyCode::Char('o'), KeyModifiers::CONTROL) => {
                // Toggle old side search
                if self.diff_find_search_old && !self.diff_find_search_new {
                    return; // must keep at least one side
                }
                self.diff_find_search_old = !self.diff_find_search_old;
                self.diff_find_current = 0;
                self.update_diff_find_matches();
                self.jump_to_nearest_diff_find();
            }
            (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                // Toggle new side search
                if self.diff_find_search_new && !self.diff_find_search_old {
                    return; // must keep at least one side
                }
                self.diff_find_search_new = !self.diff_find_search_new;
                self.diff_find_current = 0;
                self.update_diff_find_matches();
                self.jump_to_nearest_diff_find();
            }
            (KeyCode::Backspace, _) => {
                self.diff_find_query.pop();
                self.diff_find_current = 0;
                self.update_diff_find_matches();
                self.jump_to_nearest_diff_find();
            }
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                self.diff_find_query.push(c);
                self.diff_find_current = 0;
                self.update_diff_find_matches();
                self.jump_to_nearest_diff_find();
            }
            _ => {}
        }
    }

    fn update_diff_find_matches(&mut self) {
        self.diff_find_matches.clear();
        let query = self.diff_find_query.to_lowercase();
        if query.is_empty() {
            return;
        }

        let file = match self.files.get(self.nav.current_file) {
            Some(f) => f,
            None => return,
        };

        let query_chars: Vec<char> = query.chars().collect();
        let qlen = query_chars.len();

        for (row_idx, row) in file.rows.iter().enumerate() {
            if self.diff_find_search_old && !row.left.is_filler {
                let chars: Vec<char> = row.left.content.to_lowercase().chars().collect();
                if chars.len() >= qlen {
                    for i in 0..=chars.len() - qlen {
                        if chars[i..i + qlen] == query_chars[..] {
                            self.diff_find_matches.push(DiffFindMatch {
                                row: row_idx,
                                col: i,
                                len: qlen,
                                is_left: true,
                            });
                        }
                    }
                }
            }
            if self.diff_find_search_new && !row.right.is_filler {
                let chars: Vec<char> = row.right.content.to_lowercase().chars().collect();
                if chars.len() >= qlen {
                    for i in 0..=chars.len() - qlen {
                        if chars[i..i + qlen] == query_chars[..] {
                            self.diff_find_matches.push(DiffFindMatch {
                                row: row_idx,
                                col: i,
                                len: qlen,
                                is_left: false,
                            });
                        }
                    }
                }
            }
        }
    }

    fn next_diff_find_match(&mut self) {
        if self.diff_find_matches.is_empty() {
            return;
        }
        self.diff_find_current = (self.diff_find_current + 1) % self.diff_find_matches.len();
        let row = self.diff_find_matches[self.diff_find_current].row;
        self.diff_cursor = row;
        self.ensure_diff_cursor_visible(30);
    }

    fn prev_diff_find_match(&mut self) {
        if self.diff_find_matches.is_empty() {
            return;
        }
        self.diff_find_current = (self.diff_find_current + self.diff_find_matches.len() - 1)
            % self.diff_find_matches.len();
        let row = self.diff_find_matches[self.diff_find_current].row;
        self.diff_cursor = row;
        self.ensure_diff_cursor_visible(30);
    }

    fn jump_to_nearest_diff_find(&mut self) {
        if self.diff_find_matches.is_empty() {
            self.diff_find_current = 0;
            return;
        }
        let cursor = self.diff_cursor;
        let idx = self
            .diff_find_matches
            .iter()
            .position(|m| m.row >= cursor)
            .unwrap_or(0);
        self.diff_find_current = idx;
        self.diff_cursor = self.diff_find_matches[idx].row;
        self.ensure_diff_cursor_visible(30);
    }

    fn set_compare_cursor(&mut self, new_cursor: usize) {
        match &mut self.view {
            View::Compare(CompareState::PickNew {
                cursor, scroll, ..
            }) => {
                *cursor = new_cursor;
                *scroll = adjust_scroll(*scroll, new_cursor, 30);
            }
            View::Compare(CompareState::PickOld {
                cursor, scroll, ..
            }) => {
                *cursor = new_cursor;
                *scroll = adjust_scroll(*scroll, new_cursor, 30);
            }
            _ => {}
        }
    }
}

fn adjust_scroll(scroll: usize, cursor: usize, viewport: usize) -> usize {
    if cursor < scroll {
        cursor
    } else if cursor >= scroll + viewport {
        cursor.saturating_sub(viewport - 1)
    } else {
        scroll
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max.saturating_sub(3)])
    } else {
        s.to_string()
    }
}

/// Build a short context string from a CompareItem: "{hash} {date}" or label for special items.
fn short_context(item: &CompareItem) -> String {
    if item.is_special {
        return item.label.clone();
    }
    let hash = item.short_hash.as_deref().unwrap_or("???");
    let date = item
        .date
        .as_deref()
        .and_then(|d| d.split(' ').next())
        .unwrap_or("");
    format!("{} {}", hash, date)
}

/// Extract a short context from a label string (for the new_label which is already formatted).
fn short_context_from_label(label: &str) -> String {
    // Special labels like "WORKING TREE" or "STAGED (INDEX)" pass through
    if label.starts_with("WORKING") || label.starts_with("STAGED") || label.starts_with("INDEX") {
        return label.to_string();
    }
    // For commit labels formatted as "{hash} {date} {time} {subject...}", take hash + date
    let parts: Vec<&str> = label.splitn(3, ' ').collect();
    if parts.len() >= 2 {
        format!("{} {}", parts[0], parts[1])
    } else {
        label.to_string()
    }
}

fn build_old_items(new_rev: &str) -> Vec<CompareItem> {
    let mut items = Vec::new();

    // When new = WORKING TREE, INDEX is valid old side
    if new_rev == "--working-tree" {
        items.push(CompareItem {
            rev: "--index".to_string(),
            label: "INDEX".to_string(),
            is_special: true,
            short_hash: None,
            date: None,
            subject: None,
        });
    }

    // For special endpoints, any commit is valid as old side.
    // For commit endpoints, only show ancestors (git log <commit> shows that commit + ancestors).
    let revspec = if new_rev.starts_with("--") {
        None
    } else {
        Some(new_rev)
    };

    if let Ok(entries) = git::git_log_revspec(200, revspec) {
        for entry in entries {
            // Skip the new_rev itself
            if entry.full_hash == new_rev {
                continue;
            }
            items.push(CompareItem {
                rev: entry.full_hash.clone(),
                label: format!("{} {} {}", entry.short_hash, entry.date, truncate(&entry.subject, 30)),
                is_special: false,
                short_hash: Some(entry.short_hash),
                date: Some(entry.date),
                subject: Some(entry.subject),
            });
        }
    }

    items
}

fn resolve_compare_mode(old_rev: &str, new_rev: &str) -> DiffMode {
    match (old_rev, new_rev) {
        ("--index", "--working-tree") => DiffMode::Unstaged,
        (_, "--working-tree") => DiffMode::WorkingTree(old_rev.to_string()),
        (_, "--staged") => DiffMode::StagedVsCommit(old_rev.to_string()),
        _ => DiffMode::Range(format!("{old_rev}..{new_rev}")),
    }
}

/// The type of diff to perform.
#[derive(Clone)]
pub enum DiffMode {
    Range(String),
    Unstaged,
    Staged,
    WorkingTree(String),
    StagedVsCommit(String),
}

impl DiffMode {
    /// Returns a scope key for review tracking: `"old_ref:new_ref"`.
    pub fn scope_key(&self) -> String {
        match self {
            DiffMode::Range(range) => {
                let (old, new) = git::parse_git_range(range);
                let old = git::resolve_rev(&old).unwrap_or(old);
                let new = git::resolve_rev(&new).unwrap_or(new);
                format!("{old}:{new}")
            }
            DiffMode::Unstaged => "INDEX:WORKTREE".into(),
            DiffMode::Staged => {
                let head = git::resolve_rev("HEAD").unwrap_or_else(|| "HEAD".into());
                format!("{head}:INDEX")
            }
            DiffMode::WorkingTree(c) => {
                let c = git::resolve_rev(c).unwrap_or_else(|| c.clone());
                format!("{c}:WORKTREE")
            }
            DiffMode::StagedVsCommit(c) => {
                let c = git::resolve_rev(c).unwrap_or_else(|| c.clone());
                format!("{c}:INDEX")
            }
        }
    }
}

type DiffResult = (Vec<difft::DifftFile>, git::FileStats, HashMap<PathBuf, PathBuf>);

/// Run the full diff pipeline in a background thread.
fn run_diff_background(
    mode: DiffMode,
    completed: Arc<AtomicUsize>,
) -> Result<Vec<DisplayFile>, String> {
    let (files, stats, renames) = run_diff(&mode, &completed)?;
    let mut display_files = process_diff_files(files, &stats, &mode)?;

    if !renames.is_empty() {
        apply_renames(&mut display_files, &renames);
    }

    if matches!(mode, DiffMode::Unstaged | DiffMode::WorkingTree(_)) {
        let untracked = load_untracked_files();
        display_files.extend(untracked);
    }

    display_files.retain(|f| f.status != FileStatus::Unchanged);
    integrity::verify_display(&display_files)?;
    Ok(display_files)
}

/// Runs the diff and returns (difft_files, stats, renames).
fn run_diff(mode: &DiffMode, completed: &AtomicUsize) -> Result<DiffResult, String> {
    let extra_args: Vec<String> = match mode {
        DiffMode::Range(range) => {
            let (o, n) = git::parse_git_range(range);
            vec![format!("{o}..{n}")]
        }
        DiffMode::Unstaged => vec![],
        DiffMode::Staged => vec!["--cached".to_string()],
        DiffMode::WorkingTree(commit) => vec![commit.clone()],
        DiffMode::StagedVsCommit(commit) => {
            vec!["--cached".to_string(), commit.clone()]
        }
    };

    let refs: Vec<&str> = extra_args.iter().map(|s| s.as_str()).collect();

    // Run diff, stats, and renames concurrently
    let stats_refs = refs.clone();
    let renames_refs = refs.clone();

    let (files_result, stats, renames) = std::thread::scope(|s| {
        let files_handle = s.spawn(|| run_parallel_diff(&refs, mode, completed));
        let stats_handle = s.spawn(|| git::git_diff_stats(&stats_refs));
        let renames_handle = s.spawn(|| git::git_rename_map(&renames_refs));

        (
            files_handle.join().unwrap(),
            stats_handle.join().unwrap(),
            renames_handle.join().unwrap(),
        )
    });

    let files = files_result?;
    Ok((files, stats, renames))
}

/// How to fetch one side of a diff.
enum FetchMethod {
    FromRef(String),
    FromIndex,
    FromWorkingTree,
}

impl FetchMethod {
    fn fetch(&self, path: &std::path::Path) -> Option<String> {
        match self {
            Self::FromRef(r) => git::git_file_content(r, path),
            Self::FromIndex => git::git_index_content(path),
            Self::FromWorkingTree => git::working_tree_content(path),
        }
    }
}

/// Run difft per file in parallel for all diff modes.
fn run_parallel_diff(
    extra_args: &[&str],
    mode: &DiffMode,
    completed: &AtomicUsize,
) -> Result<Vec<difft::DifftFile>, String> {
    use rayon::prelude::*;

    let (old_fetch, new_fetch) = match mode {
        DiffMode::Range(range) => {
            let (old_ref, new_ref) = git::parse_git_range(range);
            (FetchMethod::FromRef(old_ref), FetchMethod::FromRef(new_ref))
        }
        DiffMode::Unstaged => (FetchMethod::FromIndex, FetchMethod::FromWorkingTree),
        DiffMode::Staged => (
            FetchMethod::FromRef("HEAD".to_string()),
            FetchMethod::FromIndex,
        ),
        DiffMode::WorkingTree(commit) => (
            FetchMethod::FromRef(commit.clone()),
            FetchMethod::FromWorkingTree,
        ),
        DiffMode::StagedVsCommit(commit) => (
            FetchMethod::FromRef(commit.clone()),
            FetchMethod::FromIndex,
        ),
    };

    let entries = git::git_changed_files(extra_args)?;
    let expected_count = entries.len();

    let expected_files: Vec<(PathBuf, String)> = entries
        .iter()
        .map(|e| (e.new_path.clone(), e.status.clone()))
        .collect();

    let tmp_dir = tempfile::TempDir::new()
        .map_err(|e| format!("Failed to create temp dir: {e}"))?;

    let results: Vec<Result<difft::DifftFile, String>> = entries
        .into_par_iter()
        .enumerate()
        .map(|(i, entry)| {
            let path_display = entry.new_path.display().to_string();

            let slot = tmp_dir.path().join(i.to_string());
            std::fs::create_dir_all(&slot)
                .map_err(|e| format!("{path_display}: temp dir: {e}"))?;

            let old_dir = slot.join("old");
            let new_dir = slot.join("new");
            std::fs::create_dir_all(&old_dir)
                .map_err(|e| format!("{path_display}: old dir: {e}"))?;
            std::fs::create_dir_all(&new_dir)
                .map_err(|e| format!("{path_display}: new dir: {e}"))?;

            let old_filename = entry
                .old_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let new_filename = entry
                .new_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();

            let old_tmp = old_dir.join(if old_filename.is_empty() {
                "file"
            } else {
                &old_filename
            });
            let new_tmp = new_dir.join(if new_filename.is_empty() {
                "file"
            } else {
                &new_filename
            });

            let old_content = if entry.status.starts_with('A') {
                String::new()
            } else {
                old_fetch.fetch(&entry.old_path).ok_or_else(|| {
                    format!("{path_display}: failed to fetch old content")
                })?
            };

            let new_content = if entry.status.starts_with('D') {
                String::new()
            } else {
                new_fetch.fetch(&entry.new_path).ok_or_else(|| {
                    format!("{path_display}: failed to fetch new content")
                })?
            };

            std::fs::write(&old_tmp, &old_content)
                .map_err(|e| format!("{path_display}: write old: {e}"))?;
            std::fs::write(&new_tmp, &new_content)
                .map_err(|e| format!("{path_display}: write new: {e}"))?;

            let output = std::process::Command::new("difft")
                .arg(&old_tmp)
                .arg(&new_tmp)
                .env("DFT_DISPLAY", "json")
                .env("DFT_UNSTABLE", "yes")
                .output()
                .map_err(|e| format!("{path_display}: difft failed to run: {e}"))?;

            let exit_code = output.status.code().unwrap_or(-1);
            if exit_code != 0 && exit_code != 1 {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!(
                    "{path_display}: difft exited with code {exit_code}: {stderr}"
                ));
            }

            let json = String::from_utf8_lossy(&output.stdout);
            if json.trim().is_empty() {
                let lang = git::language_from_ext(&entry.new_path);
                completed.fetch_add(1, Ordering::Relaxed);
                return Ok(difft::DifftFile {
                    path: entry.new_path,
                    language: lang,
                    status: difft::Status::Unchanged,
                    aligned_lines: vec![],
                    chunks: vec![],
                });
            }

            let mut parsed = difft::parse(&json)
                .map_err(|e| format!("{path_display}: JSON parse error: {e}"))?;

            if parsed.len() != 1 {
                return Err(format!(
                    "{path_display}: expected 1 file from difft, got {}",
                    parsed.len()
                ));
            }

            let mut file = parsed.remove(0);
            file.path = entry.new_path;
            completed.fetch_add(1, Ordering::Relaxed);
            Ok(file)
        })
        .collect();

    let mut all_files = Vec::with_capacity(expected_count);
    for result in results {
        all_files.push(result?);
    }

    // Post-diff integrity checks
    let expected_entries: Vec<git::ChangedEntry> = expected_files
        .iter()
        .map(|(path, status)| git::ChangedEntry {
            status: status.clone(),
            old_path: path.clone(),
            new_path: path.clone(),
        })
        .collect();
    integrity::verify(&expected_entries, &all_files)?;

    Ok(all_files)
}

/// Content fetcher strategy.
enum ContentFetcher {
    Range(String, String),
    Unstaged,
    Staged,
    WorkingTree(String),
    StagedVsCommit(String),
}

impl ContentFetcher {
    fn new(mode: &DiffMode) -> Self {
        match mode {
            DiffMode::Range(range) => {
                let (old_ref, new_ref) = git::parse_git_range(range);
                Self::Range(old_ref, new_ref)
            }
            DiffMode::Unstaged => Self::Unstaged,
            DiffMode::Staged => Self::Staged,
            DiffMode::WorkingTree(commit) => Self::WorkingTree(commit.clone()),
            DiffMode::StagedVsCommit(commit) => Self::StagedVsCommit(commit.clone()),
        }
    }

    fn fetch(
        &self,
        old_path: &std::path::Path,
        new_path: &std::path::Path,
    ) -> (Vec<String>, Vec<String>) {
        match self {
            Self::Range(old_ref, new_ref) => (
                git::into_lines(git::git_file_content(old_ref, old_path)),
                git::into_lines(git::git_file_content(new_ref, new_path)),
            ),
            Self::Unstaged => (
                git::into_lines(git::git_index_content(old_path)),
                git::into_lines(git::working_tree_content(new_path)),
            ),
            Self::Staged => (
                git::into_lines(git::git_file_content("HEAD", old_path)),
                git::into_lines(git::git_index_content(new_path)),
            ),
            Self::WorkingTree(commit) => (
                git::into_lines(git::git_file_content(commit, old_path)),
                git::into_lines(git::working_tree_content(new_path)),
            ),
            Self::StagedVsCommit(commit) => (
                git::into_lines(git::git_file_content(commit, old_path)),
                git::into_lines(git::git_index_content(new_path)),
            ),
        }
    }
}

/// Process difft files into display format.
fn process_diff_files(
    files: Vec<difft::DifftFile>,
    stats: &git::FileStats,
    mode: &DiffMode,
) -> Result<Vec<DisplayFile>, String> {
    use rayon::prelude::*;

    let fetcher = ContentFetcher::new(mode);

    let display_files: Vec<DisplayFile> = files
        .into_par_iter()
        .map(|file| {
            let file_stats = stats.get(&file.path).copied();
            let old_path = file.path.clone();
            let new_path = file.path.clone();
            let (old_lines, new_lines) = fetcher.fetch(&old_path, &new_path);
            let content_hash = compute_content_hash(&old_lines, &new_lines);
            processor::process_file(file, old_lines, new_lines, file_stats, content_hash)
        })
        .collect();

    Ok(display_files)
}

/// Apply rename detection to display files.
fn apply_renames(display_files: &mut Vec<DisplayFile>, renames: &HashMap<PathBuf, PathBuf>) {
    use std::collections::HashSet;

    let old_paths: HashSet<PathBuf> = renames.values().cloned().collect();

    display_files.retain_mut(|file| {
        if let Some(old_path) = renames.get(&file.path) {
            file.moved_from = Some(old_path.clone());
            file.status = FileStatus::Created;
        }

        if file.status == FileStatus::Deleted && old_paths.contains(&file.path) {
            return false;
        }

        true
    });
}

/// Load untracked files as created display files.
fn load_untracked_files() -> Vec<DisplayFile> {
    use rayon::prelude::*;

    let untracked = git::git_untracked_files();
    let root = git::git_root().ok();

    untracked
        .into_par_iter()
        .filter_map(|path| {
            let abs_path = root.as_ref()?.join(&path);
            let content = std::fs::read_to_string(&abs_path).ok()?;
            let new_lines: Vec<String> = content.lines().map(String::from).collect();
            let num_lines = new_lines.len() as u32;
            let language = git::language_from_ext(&path);
            let empty: Vec<String> = vec![];
            let content_hash = compute_content_hash(&empty, &new_lines);

            Some(processor::process_file(
                difft::DifftFile {
                    path,
                    language,
                    status: difft::Status::Created,
                    aligned_lines: vec![],
                    chunks: vec![],
                },
                vec![],
                new_lines,
                Some((num_lines, 0)),
                content_hash,
            ))
        })
        .collect()
}

/// Compute a content hash from old and new lines for review tracking.
fn compute_content_hash(old_lines: &[String], new_lines: &[String]) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    old_lines.hash(&mut hasher);
    new_lines.hash(&mut hasher);
    hasher.finish()
}
