//! Find bar for in-diff search: query input, match counter, old/new side toggle.

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

use crate::app::App;

pub fn render_find_bar(f: &mut Frame, area: Rect, app: &App) {
    let bg = Color::Rgb(28, 28, 34);
    let dim = Style::default().bg(bg).fg(Color::Rgb(100, 100, 110));
    let text_style = Style::default().bg(bg).fg(Color::White);
    let active_toggle = Style::default().bg(Color::Rgb(50, 50, 60)).fg(Color::White);
    let inactive_toggle = Style::default().bg(bg).fg(Color::Rgb(80, 80, 90));
    let fill_style = Style::default().bg(bg);

    // Build right side first (fixed-width: toggles + hints)
    let mut right: Vec<Span> = Vec::new();

    let old_style = if app.diff_find_search_old { active_toggle } else { inactive_toggle };
    right.push(Span::styled(" Old", old_style));
    right.push(Span::styled(" ", dim));
    let new_style = if app.diff_find_search_new { active_toggle } else { inactive_toggle };
    right.push(Span::styled("New ", new_style));

    let hints = " ^O old  ^N new  Enter next  Esc close ";
    right.push(Span::styled(hints, Style::default().bg(bg).fg(Color::Rgb(80, 80, 90))));

    let right_width: usize = right.iter().map(|s| s.content.len()).sum();

    // Build left side: query input + match counter
    let mut left: Vec<Span> = Vec::new();
    left.push(Span::styled(" / ", dim));
    left.push(Span::styled(&app.diff_find_query, text_style));
    left.push(Span::styled("_", Style::default().bg(bg).fg(Color::Rgb(80, 80, 180))));

    if !app.diff_find_query.is_empty() {
        left.push(Span::styled("  ", dim));
        let total = app.diff_find_matches.len();
        if total == 0 {
            left.push(Span::styled("no matches", dim));
        } else {
            let current = (app.diff_find_current.min(total - 1)) + 1;
            left.push(Span::styled(format!("{current}/{total}"), dim));
        }
    }

    let left_width: usize = left.iter().map(|s| s.content.len()).sum();

    // Assemble: left + fill + right
    let fill = (area.width as usize).saturating_sub(left_width + right_width);
    let mut spans = left;
    spans.push(Span::styled(" ".repeat(fill), fill_style));
    spans.extend(right);

    let line = Line::from(spans);
    let bar = ratatui::widgets::Paragraph::new(line).style(Style::default().bg(bg));
    f.render_widget(bar, area);
}
