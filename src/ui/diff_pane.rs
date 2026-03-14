//! Side-by-side diff rendering with syntax-aware highlights and cursor tracking.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

use crate::app::DiffFindMatch;
use crate::types::{DisplayFile, HighlightRegion, FULL_LINE};
use crate::ui::highlights;

/// Parameters for rendering one side of the diff.
pub struct DiffSideParams<'a> {
    pub is_left: bool,
    pub scroll: usize,
    pub h_scroll: usize,
    pub line_num_width: u16,
    pub cursor_row: usize,
    /// If active, (all matches, current match index).
    pub find_highlights: Option<(&'a [DiffFindMatch], usize)>,
}

/// Renders one side of the diff (left or right pane).
pub fn render_diff_side(
    buf: &mut Buffer,
    area: Rect,
    file: &DisplayFile,
    params: &DiffSideParams<'_>,
) {
    let DiffSideParams { is_left, scroll, h_scroll, line_num_width, cursor_row, .. } = *params;
    if area.width == 0 || area.height == 0 {
        return;
    }

    // Reserve 1 col for cursor marker on left pane only
    let marker_width: u16 = if is_left { 1 } else { 0 };
    let content_width = area.width.saturating_sub(line_num_width + 1 + marker_width) as usize;

    for y in 0..area.height {
        let row_idx = scroll + y as usize;
        let screen_y = area.y + y;

        if row_idx >= file.rows.len() {
            break;
        }

        let row = &file.rows[row_idx];
        let side = if is_left { &row.left } else { &row.right };
        let is_cursor_line = row_idx == cursor_row;

        // Cursor marker (left pane only)
        if is_left {
            let marker_style = if is_cursor_line {
                highlights::cursor_marker_style()
            } else {
                ratatui::style::Style::default()
            };
            let marker_char = if is_cursor_line { '▎' } else { ' ' };
            buf[(area.x, screen_y)]
                .set_char(marker_char)
                .set_style(marker_style);
        }

        // Line number
        let line_num_x = area.x + marker_width;
        let line_num_area = Rect {
            x: line_num_x,
            y: screen_y,
            width: line_num_width,
            height: 1,
        };
        render_line_number(buf, line_num_area, file, row_idx, is_left, side.is_filler, is_cursor_line);

        // Separator
        let sep_x = line_num_x + line_num_width;
        if sep_x < area.x + area.width {
            let sep_style = if is_cursor_line {
                highlights::cursor_line_style()
            } else {
                highlights::border_style()
            };
            buf[(sep_x, screen_y)]
                .set_char(' ')
                .set_style(sep_style);
        }

        // Content area
        let content_x = sep_x + 1;
        let content_area = Rect {
            x: content_x,
            y: screen_y,
            width: content_width as u16,
            height: 1,
        };

        if side.is_filler {
            render_filler_line(buf, content_area);
        } else {
            let other_side = if is_left { &row.right } else { &row.left };
            let paired_has_changes = !other_side.highlights.is_empty() || other_side.is_filler;

            // Collect find matches for this row+side
            let row_find_matches: Vec<(usize, usize, bool)> = if let Some((matches, current_idx)) = params.find_highlights {
                matches
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| m.row == row_idx && m.is_left == is_left)
                    .map(|(i, m)| (m.col, m.len, i == current_idx))
                    .collect()
            } else {
                Vec::new()
            };

            render_content_line(buf, content_area, side, is_left, h_scroll, is_cursor_line, paired_has_changes, &row_find_matches);
        }
    }
}

fn render_line_number(
    buf: &mut Buffer,
    area: Rect,
    file: &DisplayFile,
    row_idx: usize,
    is_left: bool,
    is_filler: bool,
    is_cursor_line: bool,
) {
    if is_filler {
        let filler_str = " ".repeat(area.width as usize);
        let line = Line::from(Span::styled(filler_str, highlights::filler_style()));
        buf.set_line(area.x, area.y, &line, area.width);
        return;
    }

    let aligned = file.aligned_lines.get(row_idx);
    let line_num = if is_left {
        aligned.and_then(|a| a.0)
    } else {
        aligned.and_then(|a| a.1)
    };

    if let Some(num) = line_num {
        let display_num = num + 1; // 1-indexed display
        let num_str = format!("{:>width$}", display_num, width = area.width as usize);

        let side = if is_left {
            &file.rows[row_idx].left
        } else {
            &file.rows[row_idx].right
        };
        let mut style = if !side.highlights.is_empty() {
            highlights::line_number_changed_style()
        } else {
            highlights::line_number_style()
        };

        if is_cursor_line {
            style = style.bg(highlights::cursor_line_style().bg.unwrap_or(ratatui::style::Color::Reset));
        }

        let line = Line::from(Span::styled(num_str, style));
        buf.set_line(area.x, area.y, &line, area.width);
    } else {
        let mut style = highlights::line_number_style();
        if is_cursor_line {
            style = style.bg(highlights::cursor_line_style().bg.unwrap_or(ratatui::style::Color::Reset));
        }
        let empty = " ".repeat(area.width as usize);
        let line = Line::from(Span::styled(empty, style));
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

fn render_filler_line(buf: &mut Buffer, area: Rect) {
    let filler_char = '/';
    let style = highlights::filler_style();

    for x in 0..area.width {
        let col = area.x + x;
        // Alternating pattern for visual distinction
        if (x % 2) == 0 {
            buf[(col, area.y)].set_char(filler_char).set_style(style);
        } else {
            buf[(col, area.y)].set_char(' ').set_style(style);
        }
    }
}

fn render_content_line(
    buf: &mut Buffer,
    area: Rect,
    side: &crate::types::Side,
    is_left: bool,
    h_scroll: usize,
    is_cursor_line: bool,
    paired_has_changes: bool,
    find_matches: &[(usize, usize, bool)], // (col, len, is_current)
) {
    let content = &side.content;
    let has_changes = !side.highlights.is_empty();

    // Determine base style
    let base_style = if has_changes {
        let diff_style = if is_left {
            highlights::deleted_bg_style()
        } else {
            highlights::added_bg_style()
        };
        if is_cursor_line {
            let cursor_bg = if is_left {
                ratatui::style::Color::Rgb(80, 30, 30)
            } else {
                ratatui::style::Color::Rgb(30, 65, 30)
            };
            diff_style.bg(cursor_bg)
        } else {
            diff_style
        }
    } else if paired_has_changes {
        // Other side has changes but this side doesn't — show subtle bg
        // so the user knows this line is part of a changed pair
        let paired_style = if is_left {
            highlights::deleted_bg_style()
        } else {
            highlights::added_bg_style()
        };
        if is_cursor_line {
            let cursor_bg = if is_left {
                ratatui::style::Color::Rgb(80, 30, 30)
            } else {
                ratatui::style::Color::Rgb(30, 65, 30)
            };
            paired_style.bg(cursor_bg)
        } else {
            paired_style
        }
    } else if is_cursor_line {
        highlights::unchanged_style().bg(highlights::cursor_line_style().bg.unwrap_or(ratatui::style::Color::Reset))
    } else {
        highlights::unchanged_style()
    };

    // Fill background first
    for x in 0..area.width {
        buf[(area.x + x, area.y)].set_char(' ').set_style(base_style);
    }

    // Apply content with h_scroll
    let chars: Vec<char> = content.chars().collect();
    let visible_start = h_scroll;

    for x in 0..area.width as usize {
        let char_idx = visible_start + x;
        if char_idx >= chars.len() {
            break;
        }

        let ch = chars[char_idx];
        let byte_pos = content
            .char_indices()
            .nth(char_idx)
            .map(|(i, _)| i as u32)
            .unwrap_or(0);

        // Find applicable highlight from difftastic
        let (in_changed_region, syntax_fg) = find_highlight_info(&side.highlights, byte_pos);

        let mut cell_style = if in_changed_region && has_changes {
            // Apply emphasized (brighter) background for the exact changed characters
            let emphasis_bg = if is_cursor_line {
                if is_left {
                    ratatui::style::Color::Rgb(160, 50, 50)
                } else {
                    ratatui::style::Color::Rgb(50, 125, 50)
                }
            } else if is_left {
                highlights::DELETED_EMPHASIS_BG
            } else {
                highlights::ADDED_EMPHASIS_BG
            };
            base_style.bg(emphasis_bg)
        } else {
            base_style
        };

        if let Some(fg) = syntax_fg {
            cell_style = cell_style.fg(fg);
        }

        let col = area.x + x as u16;
        if col < area.x + area.width {
            buf[(col, area.y)].set_char(ch).set_style(cell_style);
        }
    }

    // Apply find-in-diff highlights on top
    for &(match_col, match_len, is_current) in find_matches {
        let hl_style = if is_current {
            highlights::find_current_style()
        } else {
            highlights::find_match_style()
        };
        for offset in 0..match_len {
            let char_idx = match_col + offset;
            if char_idx < visible_start {
                continue;
            }
            let screen_x = char_idx - visible_start;
            let col = area.x + screen_x as u16;
            if col < area.x + area.width {
                let existing = buf[(col, area.y)].symbol().chars().next().unwrap_or(' ');
                buf[(col, area.y)].set_char(existing).set_style(hl_style);
            }
        }
    }
}

/// Checks if a byte position is in a changed region and finds its syntax color.
/// Returns (is_in_changed_region, optional_syntax_fg_color).
fn find_highlight_info(highlights: &[HighlightRegion], byte_pos: u32) -> (bool, Option<ratatui::style::Color>) {
    for region in highlights {
        let in_region = if region.end == FULL_LINE {
            true
        } else {
            byte_pos >= region.start && byte_pos < region.end as u32
        };

        if in_region {
            let fg = region.highlight.map(highlights::syntax_fg);
            return (true, fg);
        }
    }
    (false, None)
}

/// Calculates the width needed for line numbers.
pub fn line_num_width(file: &DisplayFile) -> u16 {
    let max_line = file.rows.len();
    let digits = if max_line == 0 {
        1
    } else {
        (max_line as f64).log10().floor() as u16 + 1
    };
    digits.max(3) // minimum 3 chars
}
