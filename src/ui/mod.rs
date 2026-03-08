//! Top-level draw dispatch and shared UI views (loading, error, help).

pub mod compare;
pub mod diff_pane;
pub mod highlights;
pub mod layout;
pub mod log_view;
pub mod scrollbar;
pub mod status_bar;
pub mod tree_pane;

use ratatui::Frame;

use crate::app::{App, View};

pub fn draw(f: &mut Frame, app: &App) {
    match &app.view {
        View::Log => log_view::draw(f, app),
        View::Diff => layout::draw_diff_layout(f, app),
        View::Compare(_) => compare::draw(f, app),
        View::Loading(msg) => draw_loading(f, msg),
        View::Error(msg) => draw_error(f, msg),
        View::Help => draw_help(f),
    }
}

fn draw_loading(f: &mut Frame, msg: &str) {
    use ratatui::layout::{Constraint, Layout};
    use ratatui::style::{Color, Style};
    use ratatui::widgets::{Block, Borders, Paragraph};

    let area = f.area();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(" re ")
        .style(Style::default().fg(Color::White));

    let paragraph = Paragraph::new(msg.to_string())
        .style(Style::default().fg(Color::Yellow))
        .block(block)
        .alignment(ratatui::layout::Alignment::Center);

    let vertical = Layout::vertical([
        Constraint::Percentage(40),
        Constraint::Length(3),
        Constraint::Percentage(40),
    ])
    .split(area);

    f.render_widget(paragraph, vertical[1]);
}

fn draw_error(f: &mut Frame, msg: &str) {
    use ratatui::style::{Color, Style};
    use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

    let area = f.area();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(" Error ")
        .style(Style::default().fg(Color::Red));

    let paragraph = Paragraph::new(msg.to_string())
        .style(Style::default().fg(Color::Red))
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

fn draw_help(f: &mut Frame) {
    use ratatui::style::{Color, Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

    let key_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::White);
    let header_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);

    let lines = vec![
        Line::from(Span::styled("Key Bindings", header_style)),
        Line::from(""),
        Line::from(Span::styled("Navigation", header_style)),
        Line::from(vec![
            Span::styled("j/Down      ", key_style),
            Span::styled("Scroll down", desc_style),
        ]),
        Line::from(vec![
            Span::styled("k/Up        ", key_style),
            Span::styled("Scroll up", desc_style),
        ]),
        Line::from(vec![
            Span::styled("Shift+Down  ", key_style),
            Span::styled("Scroll down 5 lines", desc_style),
        ]),
        Line::from(vec![
            Span::styled("Shift+Up    ", key_style),
            Span::styled("Scroll up 5 lines", desc_style),
        ]),
        Line::from(vec![
            Span::styled("Ctrl+d      ", key_style),
            Span::styled("Half page down", desc_style),
        ]),
        Line::from(vec![
            Span::styled("Ctrl+u      ", key_style),
            Span::styled("Half page up", desc_style),
        ]),
        Line::from(vec![
            Span::styled("gg          ", key_style),
            Span::styled("Top of file", desc_style),
        ]),
        Line::from(vec![
            Span::styled("G           ", key_style),
            Span::styled("Bottom of file", desc_style),
        ]),
        Line::from(vec![
            Span::styled("h/l         ", key_style),
            Span::styled("Scroll left/right", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("Diff Navigation", header_style)),
        Line::from(vec![
            Span::styled("Ctrl+S+Down ", key_style),
            Span::styled("Next hunk", desc_style),
        ]),
        Line::from(vec![
            Span::styled("Ctrl+S+Up   ", key_style),
            Span::styled("Previous hunk", desc_style),
        ]),
        Line::from(vec![
            Span::styled("]f          ", key_style),
            Span::styled("Next file", desc_style),
        ]),
        Line::from(vec![
            Span::styled("[f          ", key_style),
            Span::styled("Previous file", desc_style),
        ]),
        Line::from(vec![
            Span::styled("Tab         ", key_style),
            Span::styled("Toggle tree/diff focus", desc_style),
        ]),
        Line::from(""),
        Line::from(Span::styled("General", header_style)),
        Line::from(vec![
            Span::styled("Enter       ", key_style),
            Span::styled("Select / view diff", desc_style),
        ]),
        Line::from(vec![
            Span::styled("c           ", key_style),
            Span::styled("Compare two endpoints (from log)", desc_style),
        ]),
        Line::from(vec![
            Span::styled("/           ", key_style),
            Span::styled("Search (log/compare)", desc_style),
        ]),
        Line::from(vec![
            Span::styled("q/Esc       ", key_style),
            Span::styled("Quit / back", desc_style),
        ]),
        Line::from(vec![
            Span::styled("?           ", key_style),
            Span::styled("Toggle this help", desc_style),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(" Help ")
        .style(Style::default().fg(Color::White));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });

    // Center the help overlay
    let area = f.area();
    let h_margin = area.width.saturating_sub(60) / 2;
    let v_margin = area.height.saturating_sub(30) / 2;
    let help_area = ratatui::layout::Rect {
        x: area.x + h_margin,
        y: area.y + v_margin,
        width: area.width.saturating_sub(h_margin * 2).min(60),
        height: area.height.saturating_sub(v_margin * 2).min(30),
    };

    // Clear the area behind the help overlay
    f.render_widget(ratatui::widgets::Clear, help_area);
    f.render_widget(paragraph, help_area);
}
