//! File tree sidebar: builds, flattens, and renders the directory tree.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::types::{DisplayFile, FileStatus};
use crate::ui::highlights;

/// A node in the file tree.
#[derive(Debug, Clone)]
pub struct TreeNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub file_idx: Option<usize>,
    pub status: Option<FileStatus>,
    pub additions: u32,
    pub deletions: u32,
    pub children: Vec<TreeNode>,
    pub expanded: bool,
    pub moved_from: Option<String>,
}

/// Builds a tree from a flat file list.
pub fn build_tree(files: &[DisplayFile]) -> Vec<TreeNode> {
    let mut root_children: Vec<TreeNode> = Vec::new();

    for (idx, file) in files.iter().enumerate() {
        let path_str = file.path.to_string_lossy().to_string();
        let parts: Vec<&str> = path_str.split('/').collect();
        insert_into_tree(&mut root_children, &parts, idx, file, 0);
    }

    // Flatten single-child directories
    flatten_single_child(&mut root_children);

    // Propagate stats
    propagate_stats(&mut root_children);

    // Sort: dirs first, then alphabetical
    sort_tree(&mut root_children);

    root_children
}

fn insert_into_tree(
    nodes: &mut Vec<TreeNode>,
    parts: &[&str],
    file_idx: usize,
    file: &DisplayFile,
    _depth: u16,
) {
    if parts.is_empty() {
        return;
    }

    let name = parts[0];
    let is_last = parts.len() == 1;

    let existing = nodes.iter().position(|n| n.name == name && n.is_dir != is_last);

    if let Some(pos) = existing {
        if !is_last {
            insert_into_tree(&mut nodes[pos].children, &parts[1..], file_idx, file, _depth + 1);
        }
    } else {
        let mut node = TreeNode {
            name: name.to_string(),
            path: parts.join("/"),
            is_dir: !is_last,
            file_idx: if is_last { Some(file_idx) } else { None },
            status: if is_last { Some(file.status) } else { None },
            additions: if is_last { file.additions } else { 0 },
            deletions: if is_last { file.deletions } else { 0 },
            children: Vec::new(),
            expanded: true,
            moved_from: file.moved_from.as_ref().map(|p| p.to_string_lossy().to_string()),
        };

        if !is_last {
            insert_into_tree(&mut node.children, &parts[1..], file_idx, file, _depth + 1);
        }

        nodes.push(node);
    }
}

fn flatten_single_child(nodes: &mut [TreeNode]) {
    for node in nodes.iter_mut() {
        flatten_single_child(&mut node.children);

        while node.children.len() == 1 && node.children[0].is_dir {
            let child = node.children.remove(0);
            node.name = format!("{}/{}", node.name, child.name);
            node.path = child.path;
            node.children = child.children;
        }
    }
}

fn propagate_stats(nodes: &mut [TreeNode]) -> (u32, u32) {
    let mut total_add = 0u32;
    let mut total_del = 0u32;

    for node in nodes.iter_mut() {
        if node.is_dir {
            let (add, del) = propagate_stats(&mut node.children);
            node.additions = add;
            node.deletions = del;
        }
        total_add += node.additions;
        total_del += node.deletions;
    }

    (total_add, total_del)
}

fn sort_tree(nodes: &mut [TreeNode]) {
    nodes.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            return b.is_dir.cmp(&a.is_dir); // dirs first
        }
        a.name.to_lowercase().cmp(&b.name.to_lowercase())
    });

    for node in nodes.iter_mut() {
        if node.is_dir {
            sort_tree(&mut node.children);
        }
    }
}

/// Flattens the tree into visible lines for rendering.
pub fn flatten_visible(nodes: &[TreeNode], depth: u16) -> Vec<FlatNode> {
    let mut result = Vec::new();

    for node in nodes {
        result.push(FlatNode {
            name: node.name.clone(),
            is_dir: node.is_dir,
            file_idx: node.file_idx,
            status: node.status,
            additions: node.additions,
            deletions: node.deletions,
            depth,
            expanded: node.expanded,
            moved_from: node.moved_from.clone(),
        });

        if node.is_dir && node.expanded {
            result.extend(flatten_visible(&node.children, depth + 1));
        }
    }

    result
}

/// A flattened tree node for rendering.
#[derive(Debug, Clone)]
pub struct FlatNode {
    pub name: String,
    pub is_dir: bool,
    pub file_idx: Option<usize>,
    pub status: Option<FileStatus>,
    pub additions: u32,
    pub deletions: u32,
    pub depth: u16,
    pub expanded: bool,
    pub moved_from: Option<String>,
}

/// Parameters for rendering the tree pane.
pub struct TreeRenderParams {
    pub current_file_idx: usize,
    pub tree_scroll: usize,
    pub tree_focused: bool,
    pub tree_cursor: usize,
    pub total_additions: u32,
    pub total_deletions: u32,
    pub file_count: usize,
}

/// Renders the tree pane.
pub fn render_tree(
    buf: &mut Buffer,
    area: Rect,
    flat_nodes: &[FlatNode],
    params: &TreeRenderParams,
) {
    let TreeRenderParams {
        current_file_idx,
        tree_scroll,
        tree_focused,
        tree_cursor,
        total_additions,
        total_deletions,
        file_count,
    } = *params;
    if area.width < 4 || area.height < 3 {
        return;
    }

    // Header: stats summary
    let header_y = area.y;
    let stats_text = format!("{} files  +{} -{}", file_count, total_additions, total_deletions);
    let padding = (area.width as usize).saturating_sub(stats_text.len()) / 2;

    // Render stats with colors
    let mut x = area.x + padding as u16;
    let files_part = format!("{} files  ", file_count);
    let add_part = format!("+{}", total_additions);
    let del_part = format!(" -{}", total_deletions);

    for ch in files_part.chars() {
        if x < area.x + area.width {
            buf[(x, header_y)]
                .set_char(ch)
                .set_style(Style::default().fg(ratatui::style::Color::White));
            x += 1;
        }
    }
    for ch in add_part.chars() {
        if x < area.x + area.width {
            buf[(x, header_y)]
                .set_char(ch)
                .set_style(highlights::additions_style());
            x += 1;
        }
    }
    for ch in del_part.chars() {
        if x < area.x + area.width {
            buf[(x, header_y)]
                .set_char(ch)
                .set_style(highlights::deletions_style());
            x += 1;
        }
    }

    // Separator line
    let sep_y = area.y + 1;
    for x in area.x..area.x + area.width {
        buf[(x, sep_y)]
            .set_char('─')
            .set_style(highlights::border_style());
    }

    // File tree entries
    let tree_area_start = area.y + 2;
    let tree_area_height = area.height.saturating_sub(2);

    for y in 0..tree_area_height {
        let node_idx = tree_scroll + y as usize;
        let screen_y = tree_area_start + y;

        if node_idx >= flat_nodes.len() {
            break;
        }

        let node = &flat_nodes[node_idx];
        let is_current = node.file_idx == Some(current_file_idx);
        let is_cursor = tree_focused && node_idx == tree_cursor;

        let line = render_tree_node(node, area.width as usize, is_current, is_cursor);
        buf.set_line(area.x, screen_y, &line, area.width);
    }
}

fn render_tree_node(node: &FlatNode, _max_width: usize, is_current: bool, is_cursor: bool) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    // Left bar indicator for cursor
    if is_cursor {
        spans.push(Span::styled("▌", highlights::tree_cursor_bar_style()));
    } else {
        spans.push(Span::raw(" "));
    }

    // Indentation
    let indent = "  ".repeat(node.depth as usize);
    spans.push(Span::raw(indent));

    // Icon
    let icon = if node.is_dir {
        if node.expanded { "▾ " } else { "▸ " }
    } else {
        "  "
    };

    let icon_style = if node.is_dir {
        highlights::tree_directory_style()
    } else {
        Style::default()
    };
    spans.push(Span::styled(icon.to_string(), icon_style));

    // Name
    if let Some(ref moved_from) = node.moved_from {
        spans.push(Span::styled(
            moved_from.clone(),
            Style::default().fg(ratatui::style::Color::Red),
        ));
        spans.push(Span::raw(" -> "));
        spans.push(Span::styled(
            node.name.clone(),
            Style::default().fg(ratatui::style::Color::Green),
        ));
    } else {
        let name_style = if let Some(status) = node.status {
            Style::default().fg(highlights::status_color(&status))
        } else if node.is_dir {
            highlights::tree_directory_style()
        } else {
            Style::default()
        };
        spans.push(Span::styled(node.name.clone(), name_style));
    }

    // Stats
    if node.additions > 0 || node.deletions > 0 {
        spans.push(Span::raw(" "));
        if node.additions > 0 {
            spans.push(Span::styled(
                format!("+{}", node.additions),
                highlights::additions_style(),
            ));
            if node.deletions > 0 {
                spans.push(Span::raw(" "));
            }
        }
        if node.deletions > 0 {
            spans.push(Span::styled(
                format!("-{}", node.deletions),
                highlights::deletions_style(),
            ));
        }
    }

    // New file tag
    if node.status == Some(FileStatus::Created) && node.moved_from.is_none() {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            "[NEW]",
            Style::default().fg(ratatui::style::Color::Green),
        ));
    }

    let mut line = Line::from(spans);

    // Background for current/cursor
    if is_cursor {
        line = line.style(highlights::selected_style());
    } else if is_current {
        line = line.style(highlights::tree_current_style());
    }

    line
}

/// Toggle expand/collapse for a directory node at cursor position.
pub fn toggle_node(nodes: &mut [TreeNode], flat_idx: usize) {
    let mut count = 0;
    toggle_node_recursive(nodes, flat_idx, &mut count);
}

fn toggle_node_recursive(nodes: &mut [TreeNode], target: usize, count: &mut usize) -> bool {
    for node in nodes.iter_mut() {
        if *count == target {
            if node.is_dir {
                node.expanded = !node.expanded;
            }
            return true;
        }
        *count += 1;

        if node.is_dir && node.expanded
            && toggle_node_recursive(&mut node.children, target, count)
        {
            return true;
        }
    }
    false
}
