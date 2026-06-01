#[derive(Debug, Clone)]
pub struct LineEditor {
    chars: Vec<char>,
    cursor: usize,
}

#[allow(unused)]
impl LineEditor {
    pub fn new() -> Self {
        Self {
            chars: Vec::new(),
            cursor: 0,
        }
    }

    pub fn from_str(s: &str) -> Self {
        let chars: Vec<char> = s.chars().collect();
        let cursor = chars.len();
        Self { chars, cursor }
    }

    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn len(&self) -> usize {
        self.chars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    pub fn clear(&mut self) {
        self.chars.clear();
        self.cursor = 0;
    }

    pub fn insert(&mut self, c: char) {
        self.chars.insert(self.cursor, c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.chars.remove(self.cursor - 1);
            self.cursor -= 1;
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.chars.len() {
            self.chars.remove(self.cursor);
        }
    }

    pub fn left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn right(&mut self) {
        if self.cursor < self.chars.len() {
            self.cursor += 1;
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.chars.len();
    }

    pub fn word_left(&mut self) {
        while self.cursor > 0 && !is_word_char(self.chars[self.cursor - 1]) {
            self.cursor -= 1;
        }
        while self.cursor > 0 && is_word_char(self.chars[self.cursor - 1]) {
            self.cursor -= 1;
        }
    }

    pub fn word_right(&mut self) {
        let n = self.chars.len();
        while self.cursor < n && !is_word_char(self.chars[self.cursor]) {
            self.cursor += 1;
        }
        while self.cursor < n && is_word_char(self.chars[self.cursor]) {
            self.cursor += 1;
        }
    }

    pub fn delete_word_forward(&mut self) {
        let start = self.cursor;
        self.word_right();
        let end = self.cursor;
        self.chars.drain(start..end);
        self.cursor = start;
    }

    pub fn delete_word_back(&mut self) {
        let end = self.cursor;
        self.word_left();
        let start = self.cursor;
        self.chars.drain(start..end);
    }
}

pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}
