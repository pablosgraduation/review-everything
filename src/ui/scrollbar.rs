//! Scrollbar widget with color-coded hunk markers.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::types::HunkKind;
use crate::ui::highlights;

/// Renders a scrollbar with color-coded change marks.
pub fn render_scrollbar(
    buf: &mut Buffer,
    area: Rect,
    total_rows: usize,
    scroll: usize,
    viewport_height: usize,
    hunks: &[(u32, u32, HunkKind)],
) {
    if area.width == 0 || area.height == 0 || total_rows == 0 {
        return;
    }

    let track_height = area.height as usize;

    // Fill track
    let track_color = highlights::scrollbar_track();
    for y in 0..track_height {
        buf[(area.x, area.y + y as u16)]
            .set_char(' ')
            .set_style(ratatui::style::Style::default().bg(track_color));
    }

    // Thumb position and size
    let thumb_size = ((viewport_height as f64 / total_rows as f64) * track_height as f64)
        .ceil() as usize;
    let thumb_size = thumb_size.max(1).min(track_height);
    let thumb_pos = if total_rows <= viewport_height {
        0
    } else {
        let max_scroll = total_rows - viewport_height;
        let pos = (scroll as f64 / max_scroll as f64) * (track_height - thumb_size) as f64;
        pos.round() as usize
    };

    let thumb_color = highlights::scrollbar_thumb();
    for y in thumb_pos..thumb_pos + thumb_size {
        if y < track_height {
            buf[(area.x, area.y + y as u16)]
                .set_char('▐')
                .set_style(ratatui::style::Style::default().fg(thumb_color));
        }
    }

    // Change marks - show full hunk spans on the scrollbar, colored by kind
    if total_rows > 0 {
        for &(start, end, kind) in hunks {
            let y_start = (start as f64 / total_rows as f64 * track_height as f64).floor() as usize;
            let y_end = (end as f64 / total_rows as f64 * track_height as f64).ceil() as usize;
            // Ensure at least 1 row visible
            let y_end = y_end.max(y_start + 1);

            let color = match kind {
                HunkKind::AddOnly => ratatui::style::Color::Green,
                HunkKind::DeleteOnly => ratatui::style::Color::Red,
                HunkKind::Mixed => ratatui::style::Color::Yellow,
            };
            let style = ratatui::style::Style::default().fg(color);

            for y in y_start..y_end.min(track_height) {
                buf[(area.x, area.y + y as u16)]
                    .set_char('▐')
                    .set_style(style);
            }
        }
    }
}
