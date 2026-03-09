//! Keyboard input handling and multi-key sequence resolution.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Actions that can be triggered by key input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    ScrollDown(usize),
    ScrollUp(usize),
    ScrollToTop,
    ScrollToBottom,
    ScrollLeft(usize),
    ScrollRight(usize),
    ScrollToLineStart,
    ScrollToLineEnd,
    HalfPageDown,
    HalfPageUp,
    NextHunk,
    PrevHunk,
    NextFile,
    PrevFile,
    ToggleTreeFocus,
    ToggleTree,
    Select,
    ToggleCollapse,
    StartCompare,
    StartSearch,
    ToggleReviewed,
    ClearAllReviews,
    ShowHelp,
    None,
}

/// Pending key state for multi-key sequences like `gg`, `]c`, `[c`.
#[derive(Debug, Default)]
pub struct InputState {
    pending: Option<char>,
}

impl InputState {
    pub fn handle_key(&mut self, key: KeyEvent, view: ViewContext) -> Action {
        let modifiers = key.modifiers;
        let code = key.code;

        // Handle pending sequences
        if let Some(prev) = self.pending.take() {
            return match (prev, code) {
                ('g', KeyCode::Char('g')) => Action::ScrollToTop,
                (']', KeyCode::Char('f')) if view == ViewContext::Diff => Action::NextFile,
                ('[', KeyCode::Char('f')) if view == ViewContext::Diff => Action::PrevFile,
                _ => Action::None,
            };
        }

        // Start multi-key sequences
        match code {
            KeyCode::Char('g') if modifiers.is_empty() => {
                self.pending = Some('g');
                return Action::None;
            }
            KeyCode::Char(']') if view == ViewContext::Diff && modifiers.is_empty() => {
                self.pending = Some(']');
                return Action::None;
            }
            KeyCode::Char('[') if view == ViewContext::Diff && modifiers.is_empty() => {
                self.pending = Some('[');
                return Action::None;
            }
            _ => {}
        }

        // Single-key bindings
        match (code, modifiers) {
            (KeyCode::Char('q'), KeyModifiers::NONE) | (KeyCode::Esc, _) => Action::Quit,
            (KeyCode::Char('?'), KeyModifiers::NONE) => Action::ShowHelp,

            // Scrolling
            (KeyCode::Char('j') | KeyCode::Down, KeyModifiers::NONE) => Action::ScrollDown(1),
            (KeyCode::Char('k') | KeyCode::Up, KeyModifiers::NONE) => Action::ScrollUp(1),
            (KeyCode::Down, KeyModifiers::SHIFT) => Action::ScrollDown(5),
            (KeyCode::Up, KeyModifiers::SHIFT) => Action::ScrollUp(5),
            (KeyCode::Down, m) if m.contains(KeyModifiers::CONTROL) && m.contains(KeyModifiers::SHIFT) => {
                Action::NextHunk
            }
            (KeyCode::Up, m) if m.contains(KeyModifiers::CONTROL) && m.contains(KeyModifiers::SHIFT) => {
                Action::PrevHunk
            }
            (KeyCode::Down, KeyModifiers::CONTROL) => Action::ScrollToBottom,
            (KeyCode::Up, KeyModifiers::CONTROL) => Action::ScrollToTop,
            (KeyCode::Char('G'), KeyModifiers::SHIFT) => Action::ScrollToBottom,
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => Action::HalfPageDown,
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => Action::HalfPageUp,

            // Horizontal scrolling
            (KeyCode::Char('h') | KeyCode::Left, KeyModifiers::NONE) if view == ViewContext::Diff => {
                Action::ScrollLeft(1)
            }
            (KeyCode::Char('l') | KeyCode::Right, KeyModifiers::NONE) if view == ViewContext::Diff => {
                Action::ScrollRight(1)
            }
            (KeyCode::Left, KeyModifiers::SHIFT) => Action::ScrollLeft(5),
            (KeyCode::Right, KeyModifiers::SHIFT) => Action::ScrollRight(5),
            (KeyCode::Left, KeyModifiers::CONTROL) => Action::ScrollToLineStart,
            (KeyCode::Right, KeyModifiers::CONTROL) => Action::ScrollToLineEnd,

            // Navigation
            (KeyCode::Tab, KeyModifiers::NONE)
                if view == ViewContext::Diff || view == ViewContext::Tree =>
            {
                Action::ToggleTreeFocus
            }
            (KeyCode::Char('t'), KeyModifiers::NONE)
                if view == ViewContext::Diff || view == ViewContext::Tree =>
            {
                Action::ToggleTree
            }
            (KeyCode::Enter, KeyModifiers::NONE) => Action::Select,
            (KeyCode::Char('o'), KeyModifiers::NONE) if view == ViewContext::Tree => {
                Action::ToggleCollapse
            }
            (KeyCode::Char('c'), KeyModifiers::NONE) if view == ViewContext::Log => {
                Action::StartCompare
            }
            (KeyCode::Char('/'), KeyModifiers::NONE)
                if view == ViewContext::Log || view == ViewContext::Compare =>
            {
                Action::StartSearch
            }
            (KeyCode::Char('r'), KeyModifiers::NONE)
                if view == ViewContext::Diff || view == ViewContext::Tree =>
            {
                Action::ToggleReviewed
            }
            (KeyCode::Char('R'), KeyModifiers::SHIFT)
                if view == ViewContext::Diff || view == ViewContext::Tree =>
            {
                Action::ClearAllReviews
            }

            _ => Action::None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewContext {
    Log,
    Diff,
    Tree,
    Compare,
}
