//! Three-pane layout: file tree, left diff, and right diff with scrollbar.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, BorderType};
use ratatui::Frame;

use crate::app::App;
use crate::ui::{diff_pane, highlights, scrollbar, status_bar, tree_pane, diff_find_bar};

pub fn draw_diff_layout(f: &mut Frame, app: &App) {
    let area = f.area();

    if app.files.is_empty() {
        draw_empty_state(f, area);
        return;
    }

    // Main layout: find bar (optional) + content + status bar
    let has_find_bar = app.diff_find_active;
    let main_layout = if has_find_bar {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area)
    };

    let (content_area, status_area) = if has_find_bar {
        diff_find_bar::render_find_bar(f, main_layout[0], app);
        (main_layout[1], main_layout[2])
    } else {
        (main_layout[0], main_layout[1])
    };

    // Content: tree | left diff | right diff
    if app.show_tree {
        let tree_width = app.tree_width;
        let remaining = content_area.width.saturating_sub(tree_width + 1); // +1 for border
        let half = remaining / 2;

        let h_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(tree_width),
                Constraint::Length(1), // border
                Constraint::Length(half),
                Constraint::Length(1), // border
                Constraint::Min(1),   // right pane gets remainder
            ])
            .split(content_area);

        let tree_area = h_layout[0];
        let left_border = h_layout[1];
        let left_area = h_layout[2];
        let mid_border = h_layout[3];
        let right_area = h_layout[4];

        // Render tree
        render_tree(f, tree_area, app);

        // Render border
        render_vertical_border(f, left_border);
        render_vertical_border(f, mid_border);

        // Render diff panes
        render_diff_panes(f, left_area, right_area, app);
    } else {
        let half = content_area.width / 2;
        let h_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(half),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(content_area);

        render_vertical_border(f, h_layout[1]);
        render_diff_panes(f, h_layout[0], h_layout[2], app);
    }

    // Status bar
    status_bar::render_status_bar(f, status_area, app);
}

fn render_tree(f: &mut Frame, area: Rect, app: &App) {
    let mut flat_nodes = tree_pane::flatten_visible(&app.tree_nodes, 0);

    // Wire reviewed state to flat nodes
    for node in &mut flat_nodes {
        if let Some(idx) = node.file_idx {
            node.is_reviewed = app.reviewed.contains(&idx);
        }
    }

    let total_additions: u32 = app.files.iter().map(|f| f.additions).sum();
    let total_deletions: u32 = app.files.iter().map(|f| f.deletions).sum();

    tree_pane::render_tree(
        f.buffer_mut(),
        area,
        &flat_nodes,
        &tree_pane::TreeRenderParams {
            current_file_idx: app.nav.current_file,
            tree_scroll: app.tree_scroll,
            tree_focused: app.tree_focused,
            tree_cursor: app.tree_cursor,
            total_additions,
            total_deletions,
            file_count: app.files.len(),
            reviewed_count: app.reviewed.len(),
        },
    );
}

fn render_diff_panes(f: &mut Frame, left_area: Rect, right_area: Rect, app: &App) {
    if let Some(file) = app.files.get(app.nav.current_file) {
        let line_num_w = diff_pane::line_num_width(file);
        let cursor = app.diff_cursor;

        let scroll = app.nav.scroll();
        let h_scroll = app.nav.h_scroll();

        // Collect search highlights for each side
        let find_hl = if app.diff_find_active && !app.diff_find_matches.is_empty() {
            Some((app.diff_find_matches.as_slice(), app.diff_find_current))
        } else {
            None
        };

        diff_pane::render_diff_side(
            f.buffer_mut(),
            left_area,
            file,
            &diff_pane::DiffSideParams {
                is_left: true, scroll, h_scroll, line_num_width: line_num_w, cursor_row: cursor,
                find_highlights: find_hl,
            },
        );

        // Right pane: reserve 1 col for scrollbar
        let right_content_width = right_area.width.saturating_sub(1);
        let right_content_area = Rect {
            x: right_area.x,
            y: right_area.y,
            width: right_content_width,
            height: right_area.height,
        };

        diff_pane::render_diff_side(
            f.buffer_mut(),
            right_content_area,
            file,
            &diff_pane::DiffSideParams {
                is_left: false, scroll, h_scroll, line_num_width: line_num_w, cursor_row: cursor,
                find_highlights: find_hl,
            },
        );

        // Scrollbar
        let scrollbar_area = Rect {
            x: right_area.x + right_content_width,
            y: right_area.y,
            width: 1,
            height: right_area.height,
        };
        scrollbar::render_scrollbar(
            f.buffer_mut(),
            scrollbar_area,
            file.rows.len(),
            app.nav.scroll(),
            right_area.height as usize,
            &file.hunks,
        );
    }
}

fn render_vertical_border(f: &mut Frame, area: Rect) {
    let style = highlights::border_style();
    for y in area.y..area.y + area.height {
        f.buffer_mut()[(area.x, y)]
            .set_char('│')
            .set_style(style);
    }
}

fn draw_empty_state(f: &mut Frame, area: Rect) {
    use ratatui::widgets::Paragraph;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" re ")
        .style(Style::default().fg(ratatui::style::Color::White));

    let paragraph = Paragraph::new("No changes found")
        .style(Style::default().fg(ratatui::style::Color::Rgb(120, 120, 120)))
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
