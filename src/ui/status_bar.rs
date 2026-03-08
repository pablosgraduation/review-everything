//! Bottom status bar: file path, index, hunk count, and key hints.

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

use crate::app::App;

pub fn render_status_bar(f: &mut Frame, area: Rect, app: &App) {
    let mut spans: Vec<Span> = Vec::new();

    let style = Style::default().bg(Color::Rgb(30, 30, 40)).fg(Color::White);
    let dim_style = Style::default()
        .bg(Color::Rgb(30, 30, 40))
        .fg(Color::Rgb(120, 120, 120));
    let key_style = Style::default()
        .bg(Color::Rgb(30, 30, 40))
        .fg(Color::Yellow);

    // Context info
    if let Some(ref ctx) = app.diff_context {
        spans.push(Span::styled(format!(" {} ", ctx), dim_style));
        spans.push(Span::styled("> ", dim_style));
    }

    // Current file
    if let Some(file) = app.files.get(app.nav.current_file) {
        let path = file.path.to_string_lossy();
        spans.push(Span::styled(path.to_string(), style));

        // File index
        spans.push(Span::styled(
            format!(" [{}/{}]", app.nav.current_file + 1, app.files.len()),
            dim_style,
        ));

        // Hunk info
        if !file.hunks.is_empty() {
            let current_hunk = file
                .hunks
                .iter()
                .position(|&(h, _, _)| h as usize >= app.nav.scroll())
                .unwrap_or(file.hunks.len());
            let current_hunk = if current_hunk > 0 && file.hunks.get(current_hunk).is_none_or(|&(h, _, _)| h as usize > app.nav.scroll()) {
                current_hunk
            } else {
                current_hunk + 1
            };
            spans.push(Span::styled(
                format!(" hunk {}/{}", current_hunk.min(file.hunks.len()), file.hunks.len()),
                dim_style,
            ));
        }
    }

    // Fill remaining space
    let used: usize = spans.iter().map(|s| s.content.len()).sum();
    let hints = " ? help  q quit";
    let remaining = (area.width as usize).saturating_sub(used + hints.len());
    spans.push(Span::styled(" ".repeat(remaining), style));
    spans.push(Span::styled(hints, key_style));

    let line = Line::from(spans);
    let status = ratatui::widgets::Paragraph::new(line)
        .style(Style::default().bg(Color::Rgb(30, 30, 40)));
    f.render_widget(status, area);
}
