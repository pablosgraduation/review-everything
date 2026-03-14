//! Color scheme and style definitions for the diff viewer UI.

use ratatui::style::{Color, Modifier, Style};

use crate::types::SyntaxHighlight;

/// Background color for deleted (left-side) content — line-level.
pub const DELETED_BG: Color = Color::Rgb(50, 18, 18);
/// Background color for added (right-side) content — line-level.
pub const ADDED_BG: Color = Color::Rgb(18, 40, 18);
/// Background for the specific changed characters within a deleted line.
pub const DELETED_EMPHASIS_BG: Color = Color::Rgb(100, 30, 30);
/// Background for the specific changed characters within an added line.
pub const ADDED_EMPHASIS_BG: Color = Color::Rgb(30, 80, 30);
/// Background for filler lines (striped pattern).
pub const FILLER_BG: Color = Color::Rgb(30, 30, 30);
/// Dimmed style for unchanged lines.
pub fn unchanged_style() -> Style {
    Style::default()
        .fg(Color::Rgb(120, 120, 120))
}

/// Style for deleted content background.
pub fn deleted_bg_style() -> Style {
    Style::default().bg(DELETED_BG)
}

/// Style for added content background.
pub fn added_bg_style() -> Style {
    Style::default().bg(ADDED_BG)
}

/// Style for filler lines.
pub fn filler_style() -> Style {
    Style::default()
        .bg(FILLER_BG)
        .fg(Color::Rgb(60, 60, 60))
}

/// Line number style.
pub fn line_number_style() -> Style {
    Style::default().fg(Color::Rgb(80, 80, 80))
}

/// Active line number style (in a changed region).
pub fn line_number_changed_style() -> Style {
    Style::default().fg(Color::Rgb(140, 140, 100))
}

/// File status colors for tree.
pub fn status_color(status: &crate::types::FileStatus) -> Color {
    match status {
        crate::types::FileStatus::Created => Color::Green,
        crate::types::FileStatus::Deleted => Color::Red,
        crate::types::FileStatus::Modified => Color::Yellow,
        crate::types::FileStatus::Unchanged => Color::Rgb(120, 120, 120),
    }
}

/// Foreground color for syntax highlights from difftastic.
pub fn syntax_fg(highlight: SyntaxHighlight) -> Color {
    match highlight {
        SyntaxHighlight::Keyword => Color::Rgb(198, 120, 221),  // purple
        SyntaxHighlight::String => Color::Rgb(152, 195, 121),   // green
        SyntaxHighlight::Comment => Color::Rgb(92, 99, 112),    // gray
        SyntaxHighlight::Type => Color::Rgb(229, 192, 123),     // gold
        SyntaxHighlight::Delimiter => Color::Rgb(171, 178, 191), // light gray
        SyntaxHighlight::Normal => Color::Rgb(200, 200, 200),   // white-ish
    }
}

/// Style for tree sidebar.
pub fn tree_directory_style() -> Style {
    Style::default()
        .fg(Color::Rgb(97, 175, 239))
        .add_modifier(Modifier::BOLD)
}

/// Style for the selected item.
pub fn selected_style() -> Style {
    Style::default()
        .bg(Color::Rgb(60, 60, 120))
        .add_modifier(Modifier::BOLD)
}

/// Left-edge bar indicator for the cursor line in the tree.
pub fn tree_cursor_bar_style() -> Style {
    Style::default()
        .fg(Color::Rgb(100, 140, 255))
        .bg(Color::Rgb(60, 60, 120))
}

/// Style for the tree current file highlight.
pub fn tree_current_style() -> Style {
    Style::default()
        .bg(Color::Rgb(50, 50, 56))
}


/// Style for stats numbers.
pub fn additions_style() -> Style {
    Style::default().fg(Color::Green)
}

pub fn deletions_style() -> Style {
    Style::default().fg(Color::Red)
}

/// Border style.
pub fn border_style() -> Style {
    Style::default().fg(Color::Rgb(60, 60, 60))
}

/// Title style for block borders (bright, visible).
pub fn title_style() -> Style {
    Style::default()
        .fg(Color::White)
}

/// Cursor line highlight in diff view.
pub fn cursor_line_style() -> Style {
    Style::default().bg(Color::Rgb(40, 40, 65))
}

/// Cursor line marker (left edge indicator).
pub fn cursor_marker_style() -> Style {
    Style::default()
        .bg(Color::Rgb(80, 80, 180))
        .fg(Color::Rgb(80, 80, 180))
}

/// Find match highlight style (yellow background).
pub fn find_match_style() -> Style {
    Style::default()
        .bg(Color::Rgb(120, 100, 20))
        .fg(Color::Black)
}

/// Current find match highlight style (bright orange background).
pub fn find_current_style() -> Style {
    Style::default()
        .bg(Color::Rgb(220, 160, 40))
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD)
}

/// Scrollbar track color.
pub fn scrollbar_track() -> Color {
    Color::Rgb(40, 40, 40)
}

/// Scrollbar thumb color.
pub fn scrollbar_thumb() -> Color {
    Color::Rgb(100, 100, 100)
}
