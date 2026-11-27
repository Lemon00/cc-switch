use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Cursor positions count Unicode scalar values, never byte offsets.
pub(super) struct TextEditor {
    pub lines: Vec<String>,
    pub row: usize,
    pub column: usize,
    pub multiline: bool,
    pub secret: bool,
    pub reveal: bool,
}

impl TextEditor {
    pub fn new(text: &str, multiline: bool, secret: bool) -> Self {
        Self {
            lines: text.split('\n').map(str::to_owned).collect(),
            row: 0,
            column: 0,
            multiline,
            secret,
            reveal: false,
        }
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    fn offset(&self) -> usize {
        self.lines[self.row]
            .char_indices()
            .nth(self.column)
            .map_or(self.lines[self.row].len(), |(index, _)| index)
    }

    pub fn insert(&mut self, input: &str) {
        // Normalize terminal paste; strip escape/control sequences as input data.
        for ch in input.replace("\r\n", "\n").replace('\r', "\n").chars() {
            if ch == '\n' && self.multiline {
                let offset = self.offset();
                let tail = self.lines[self.row].split_off(offset);
                self.row += 1;
                self.lines.insert(self.row, tail);
                self.column = 0;
            } else if ch == '\t' {
                self.insert("    ");
            } else if !ch.is_control() {
                let offset = self.offset();
                self.lines[self.row].insert(offset, ch);
                self.column += 1;
            }
        }
    }

    pub fn key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('u') if ctrl => {
                self.lines = vec![String::new()];
                self.row = 0;
                self.column = 0;
            }
            KeyCode::Char('a') if ctrl => self.column = 0,
            KeyCode::Char('e') if ctrl => self.column = self.lines[self.row].chars().count(),
            KeyCode::Char(ch)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.insert(&ch.to_string());
            }
            KeyCode::Enter => self.insert("\n"),
            KeyCode::Tab => self.insert("    "),
            KeyCode::F(2) if self.secret => self.reveal = !self.reveal,
            KeyCode::Home => self.column = 0,
            KeyCode::End => self.column = self.lines[self.row].chars().count(),
            KeyCode::Left if self.column > 0 => self.column -= 1,
            KeyCode::Left if self.row > 0 => {
                self.row -= 1;
                self.column = self.lines[self.row].chars().count();
            }
            KeyCode::Right if self.column < self.lines[self.row].chars().count() => {
                self.column += 1;
            }
            KeyCode::Right if self.row + 1 < self.lines.len() => {
                self.row += 1;
                self.column = 0;
            }
            KeyCode::Up | KeyCode::PageUp => {
                self.row = self
                    .row
                    .saturating_sub(if key.code == KeyCode::Up { 1 } else { 10 });
                self.column = self.column.min(self.lines[self.row].chars().count());
            }
            KeyCode::Down | KeyCode::PageDown => {
                self.row = (self.row + if key.code == KeyCode::Down { 1 } else { 10 })
                    .min(self.lines.len() - 1);
                self.column = self.column.min(self.lines[self.row].chars().count());
            }
            KeyCode::Backspace if self.column > 0 => {
                self.column -= 1;
                let offset = self.offset();
                self.lines[self.row].remove(offset);
            }
            KeyCode::Backspace if self.row > 0 => {
                let tail = self.lines.remove(self.row);
                self.row -= 1;
                self.column = self.lines[self.row].chars().count();
                self.lines[self.row].push_str(&tail);
            }
            KeyCode::Delete if self.column < self.lines[self.row].chars().count() => {
                let offset = self.offset();
                self.lines[self.row].remove(offset);
            }
            KeyCode::Delete if self.row + 1 < self.lines.len() => {
                let tail = self.lines.remove(self.row + 1);
                self.lines[self.row].push_str(&tail);
            }
            _ => {}
        }
    }
}
