//! Find bar for in-diff search: query input, match counter, old/new side toggle.

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

use crate::app::App;

pub fn render_find_bar(f: &mut Frame, area: Rect, app: &App) {
    let mut spans: Vec<Span> = Vec::new();

    let bg = Color::Rgb(28, 28, 34);
    let dim = Style::default().bg(bg).fg(Color::Rgb(100, 100, 110));
    let text_style = Style::default().bg(bg).fg(Color::White);
    let active_toggle = Style::default().bg(Color::Rgb(50, 50, 60)).fg(Color::White);
    let inactive_toggle = Style::default().bg(bg).fg(Color::Rgb(80, 80, 90));

    spans.push(Span::styled(" / ", dim));
    spans.push(Span::styled(&app.diff_find_query, text_style));
    spans.push(Span::styled("_ ", Style::default().bg(bg).fg(Color::Rgb(80, 80, 180))));

    // Old/New side toggles
    spans.push(Span::styled(" ", dim));
    let old_style = if app.diff_find_search_old { active_toggle } else { inactive_toggle };
    spans.push(Span::styled("Old", old_style));
    spans.push(Span::styled(" ", dim));
    let new_style = if app.diff_find_search_new { active_toggle } else { inactive_toggle };
    spans.push(Span::styled("New", new_style));

    // Match counter
    spans.push(Span::styled(" ", dim));
    if !app.diff_find_query.is_empty() {
        let total = app.diff_find_matches.len();
        if total == 0 {
            spans.push(Span::styled("no matches", dim));
        } else {
            let current = (app.diff_find_current.min(total - 1)) + 1;
            spans.push(Span::styled(
                format!("{current}/{total}"),
                dim,
            ));
        }
    }

    // Fill remaining space
    let used: usize = spans.iter().map(|s| s.content.len()).sum();
    let remaining = (area.width as usize).saturating_sub(used);
    spans.push(Span::styled(" ".repeat(remaining), Style::default().bg(bg)));

    let line = Line::from(spans);
    let bar = ratatui::widgets::Paragraph::new(line).style(Style::default().bg(bg));
    f.render_widget(bar, area);
}
