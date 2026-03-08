//! Commit log view: scrollable list with search filtering.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, BorderType, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::git::LogEntry;
use crate::ui::highlights;

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(Line::from(Span::styled(
            " Commit Log ",
            highlights::title_style(),
        )))
        .title_alignment(ratatui::layout::Alignment::Left)
        .style(highlights::border_style());

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.log_entries.is_empty() {
        let msg = Paragraph::new("No commits found")
            .style(Style::default().fg(Color::Rgb(120, 120, 120)))
            .alignment(ratatui::layout::Alignment::Center);
        f.render_widget(msg, inner);
        return;
    }

    // Title line
    let title_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: 1,
    };
    let title = Line::from(vec![
        Span::styled(
            " re",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " - review everything",
            Style::default().fg(Color::Rgb(120, 120, 120)),
        ),
    ]);
    f.render_widget(Paragraph::new(title).alignment(ratatui::layout::Alignment::Right), title_area);

    // Footer area: key hints or search bar
    let footer_height = 1;
    let list_area = Rect {
        x: inner.x,
        y: inner.y + 1,
        width: inner.width,
        height: inner.height.saturating_sub(2 + footer_height as u16),
    };

    let visible_height = list_area.height as usize;

    // Determine which entries to show
    let (display_indices, scroll, selected_idx) = if app.search_active {
        let indices: Vec<usize> = if app.search_query.is_empty() {
            (0..app.log_entries.len()).collect()
        } else {
            app.search_filtered.clone()
        };
        let sc = app.search_scroll;
        let sel = app.search_cursor;
        (indices, sc, sel)
    } else {
        let indices: Vec<usize> = (0..app.log_entries.len()).collect();
        (indices, app.log_scroll, app.log_cursor) // log_cursor is an index into the full list
    };

    for y in 0..visible_height {
        let filtered_pos = scroll + y;
        if filtered_pos >= display_indices.len() {
            break;
        }

        let idx = display_indices[filtered_pos];
        if idx >= app.log_entries.len() {
            break;
        }

        let entry = &app.log_entries[idx];
        let is_selected = if app.search_active {
            filtered_pos == selected_idx
        } else {
            idx == app.log_cursor
        };

        let line = render_log_entry(entry, list_area.width as usize, is_selected);
        let entry_area = Rect {
            x: list_area.x,
            y: list_area.y + y as u16,
            width: list_area.width,
            height: 1,
        };
        f.render_widget(Paragraph::new(line), entry_area);
    }

    // Footer
    let footer_area = Rect {
        x: inner.x,
        y: inner.y + inner.height.saturating_sub(1),
        width: inner.width,
        height: 1,
    };

    if app.search_active {
        render_search_bar(f, footer_area, &app.search_query, display_indices.len());
    } else {
        let footer = Line::from(vec![
            Span::styled(" Enter", Style::default().fg(Color::Yellow)),
            Span::styled(": view diff   ", Style::default().fg(Color::Rgb(120, 120, 120))),
            Span::styled("c", Style::default().fg(Color::Yellow)),
            Span::styled(": compare   ", Style::default().fg(Color::Rgb(120, 120, 120))),
            Span::styled("/", Style::default().fg(Color::Yellow)),
            Span::styled(": search   ", Style::default().fg(Color::Rgb(120, 120, 120))),
            Span::styled("q", Style::default().fg(Color::Yellow)),
            Span::styled(": quit", Style::default().fg(Color::Rgb(120, 120, 120))),
        ]);
        f.render_widget(Paragraph::new(footer), footer_area);
    }
}

fn render_search_bar(f: &mut Frame, area: Rect, query: &str, result_count: usize) {
    let count_str = format!(" [{result_count}]");
    let line = Line::from(vec![
        Span::styled(" / ", Style::default().fg(Color::Yellow)),
        Span::styled(query.to_string(), Style::default().fg(Color::White)),
        Span::styled("▌", Style::default().fg(Color::Rgb(120, 120, 120))),
        Span::styled(count_str, Style::default().fg(Color::Rgb(100, 100, 100))),
    ]);
    let bar = Paragraph::new(line).style(Style::default().bg(Color::Rgb(30, 30, 40)));
    f.render_widget(bar, area);
}

fn render_log_entry(entry: &LogEntry, width: usize, is_selected: bool) -> Line<'static> {
    let prefix = if is_selected { " > " } else { "   " };

    let hash_style = Style::default().fg(Color::Yellow);
    let date_style = Style::default().fg(Color::Rgb(120, 120, 120));
    let subject_style = Style::default().fg(Color::White);

    let mut spans = vec![
        Span::styled(prefix.to_string(), if is_selected {
            highlights::selected_style()
        } else {
            Style::default()
        }),
        Span::styled(format!("{} ", entry.short_hash), hash_style),
        Span::styled(format!("{}  ", entry.date), date_style),
    ];

    // Truncate subject if needed
    let used = prefix.len() + entry.short_hash.len() + 1 + entry.date.len() + 2;
    let remaining = width.saturating_sub(used);
    let subject = if entry.subject.len() > remaining {
        format!("{}...", &entry.subject[..remaining.saturating_sub(3)])
    } else {
        entry.subject.clone()
    };
    spans.push(Span::styled(subject, subject_style));

    let mut line = Line::from(spans);
    if is_selected {
        line = line.style(highlights::selected_style());
    }
    line
}
