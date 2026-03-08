//! Hunk and file navigation: scroll positions, hunk jumping, file switching.

use crate::types::DisplayFile;

/// Navigation state for diff view.
pub struct NavState {
    /// Index into the file list.
    pub current_file: usize,
    /// Current scroll position (row index) per file.
    pub scroll_positions: Vec<usize>,
    /// Current horizontal scroll offset per file.
    pub h_scroll_positions: Vec<usize>,
}

impl NavState {
    pub fn new(file_count: usize) -> Self {
        Self {
            current_file: 0,
            scroll_positions: vec![0; file_count],
            h_scroll_positions: vec![0; file_count],
        }
    }

    pub fn scroll(&self) -> usize {
        self.scroll_positions.get(self.current_file).copied().unwrap_or(0)
    }

    pub fn h_scroll(&self) -> usize {
        self.h_scroll_positions.get(self.current_file).copied().unwrap_or(0)
    }

    pub fn set_scroll(&mut self, val: usize) {
        if let Some(s) = self.scroll_positions.get_mut(self.current_file) {
            *s = val;
        }
    }

    pub fn set_h_scroll(&mut self, val: usize) {
        if let Some(s) = self.h_scroll_positions.get_mut(self.current_file) {
            *s = val;
        }
    }

    /// Next hunk in current file. Returns the row index or None if wrapping to next file.
    pub fn next_hunk(&self, files: &[DisplayFile]) -> Option<HunkJump> {
        let file = files.get(self.current_file)?;
        let current_scroll = self.scroll();

        // Find the next hunk start after current scroll position
        for &(start, _, _) in &file.hunks {
            if (start as usize) > current_scroll {
                return Some(HunkJump::SameFile(start as usize));
            }
        }

        // Wrap to next file's first hunk
        for offset in 1..files.len() {
            let idx = (self.current_file + offset) % files.len();
            if let Some(&(first, _, _)) = files[idx].hunks.first() {
                return Some(HunkJump::NextFile(idx, first as usize));
            }
        }

        None
    }

    /// Previous hunk in current file.
    pub fn prev_hunk(&self, files: &[DisplayFile]) -> Option<HunkJump> {
        let file = files.get(self.current_file)?;
        let current_scroll = self.scroll();

        // Find the previous hunk start before current scroll position
        for &(start, _, _) in file.hunks.iter().rev() {
            if (start as usize) < current_scroll {
                return Some(HunkJump::SameFile(start as usize));
            }
        }

        // Wrap to previous file's last hunk
        for offset in 1..files.len() {
            let idx = (self.current_file + files.len() - offset) % files.len();
            if let Some(&(last, _, _)) = files[idx].hunks.last() {
                return Some(HunkJump::NextFile(idx, last as usize));
            }
        }

        None
    }

    pub fn next_file(&mut self, file_count: usize) -> bool {
        if file_count == 0 {
            return false;
        }
        if self.current_file + 1 < file_count {
            self.current_file += 1;
            true
        } else {
            false
        }
    }

    pub fn prev_file(&mut self) -> bool {
        if self.current_file > 0 {
            self.current_file -= 1;
            true
        } else {
            false
        }
    }

    pub fn go_to_file(&mut self, idx: usize, file_count: usize) {
        if idx < file_count {
            self.current_file = idx;
        }
    }

    /// Auto-scroll to first hunk when opening a file.
    pub fn auto_scroll_to_first_hunk(&mut self, files: &[DisplayFile]) {
        if let Some(file) = files.get(self.current_file)
            && let Some(&(first, _, _)) = file.hunks.first()
        {
            self.set_scroll(first as usize);
        }
    }
}

pub enum HunkJump {
    SameFile(usize),
    NextFile(usize, usize),
}
