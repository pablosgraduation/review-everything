//! Compare endpoint picker: two-step flow to select old/new revisions.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, BorderType, Paragraph};
use ratatui::Frame;

use crate::app::{App, CompareState};
use crate::ui::highlights;

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();

    let compare = match &app.view {
        crate::app::View::Compare(c) => c,
        _ => return,
    };

    let (title, context) = match compare {
        CompareState::PickNew { .. } => ("Compare: select NEW endpoint", None),
        CompareState::PickOld { new_label, .. } => {
            ("Compare: select OLD endpoint", Some(format!("against: {new_label}")))
        }
    };

    let title_spans = if let Some(ref ctx) = context {
        Line::from(vec![
            Span::styled(format!(" {} ", title), highlights::title_style()),
            Span::styled(format!(" ({}) ", ctx), Style::default().fg(Color::Rgb(160, 160, 160))),
        ])
    } else {
        Line::from(Span::styled(format!(" {} ", title), highlights::title_style()))
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(title_spans)
        .style(highlights::border_style());

    let inner = block.inner(area);
    f.render_widget(block, area);

    let (items, cursor, scroll) = match compare {
        CompareState::PickNew {
            items,
            cursor,
            scroll,
        } => (items, *cursor, *scroll),
        CompareState::PickOld {
            items,
            cursor,
            scroll,
            ..
        } => (items, *cursor, *scroll),
    };

    // Reserve space for search bar at bottom if active
    let list_height = if app.search_active {
        inner.height.saturating_sub(1)
    } else {
        inner.height
    };
    let visible_height = list_height as usize;

    // Determine which items to show
    let (display_indices, eff_scroll, eff_selected) = if app.search_active {
        let indices: Vec<usize> = if app.search_query.is_empty() {
            (0..items.len()).collect()
        } else {
            app.search_filtered.clone()
        };
        (indices, app.search_scroll, app.search_cursor)
    } else {
        let indices: Vec<usize> = (0..items.len()).collect();
        (indices, scroll, cursor)
    };

    for y in 0..visible_height {
        let filtered_pos = eff_scroll + y;
        if filtered_pos >= display_indices.len() {
            break;
        }

        let idx = display_indices[filtered_pos];
        if idx >= items.len() {
            break;
        }

        let item = &items[idx];
        let is_selected = if app.search_active {
            filtered_pos == eff_selected
        } else {
            idx == cursor
        };

        let line = render_compare_item(item, inner.width as usize, is_selected);
        let entry_area = Rect {
            x: inner.x,
            y: inner.y + y as u16,
            width: inner.width,
            height: 1,
        };
        f.render_widget(Paragraph::new(line), entry_area);
    }

    // Search bar
    if app.search_active {
        let search_area = Rect {
            x: inner.x,
            y: inner.y + list_height,
            width: inner.width,
            height: 1,
        };
        let count_str = format!(" [{}]", display_indices.len());
        let line = Line::from(vec![
            Span::styled(" / ", Style::default().fg(Color::Yellow)),
            Span::styled(app.search_query.clone(), Style::default().fg(Color::White)),
            Span::styled("▌", Style::default().fg(Color::Rgb(120, 120, 120))),
            Span::styled(count_str, Style::default().fg(Color::Rgb(100, 100, 100))),
        ]);
        let bar = Paragraph::new(line).style(Style::default().bg(Color::Rgb(30, 30, 40)));
        f.render_widget(bar, search_area);
    }
}

fn render_compare_item(
    item: &crate::app::CompareItem,
    width: usize,
    is_selected: bool,
) -> Line<'static> {
    let prefix = if is_selected { " > " } else { "   " };

    let mut spans = vec![Span::styled(
        prefix.to_string(),
        if is_selected {
            highlights::selected_style()
        } else {
            Style::default()
        },
    )];

    if item.is_special {
        spans.push(Span::styled(
            item.label.clone(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
    } else {
        spans.push(Span::styled(
            format!("{} ", item.short_hash.as_deref().unwrap_or("")),
            Style::default().fg(Color::Yellow),
        ));
        if let Some(ref date) = item.date {
            spans.push(Span::styled(
                format!("{date}  "),
                Style::default().fg(Color::Rgb(120, 120, 120)),
            ));
        }
        if let Some(ref subject) = item.subject {
            let used: usize = spans.iter().map(|s| s.content.len()).sum();
            let remaining = width.saturating_sub(used);
            let subj = if subject.len() > remaining {
                format!("{}...", &subject[..remaining.saturating_sub(3)])
            } else {
                subject.clone()
            };
            spans.push(Span::styled(subj, Style::default().fg(Color::White)));
        }
    }

    let mut line = Line::from(spans);
    if is_selected {
        line = line.style(highlights::selected_style());
    }
    line
}
