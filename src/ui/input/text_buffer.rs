use super::line_editor::is_word_char;

#[derive(Debug, Clone)]
pub struct TextBuffer {
    lines: Vec<Vec<char>>,
    cursor: (usize, usize),
}

impl TextBuffer {
    pub fn new() -> Self {
        Self {
            lines: vec![Vec::new()],
            cursor: (0, 0),
        }
    }

    pub fn from_str(s: &str) -> Self {
        let lines: Vec<Vec<char>> = if s.is_empty() {
            vec![Vec::new()]
        } else {
            s.split('\n').map(|l| l.chars().collect()).collect()
        };
        let last_row = lines.len() - 1;
        let col = lines[last_row].len();
        Self {
            lines,
            cursor: (last_row, col),
        }
    }

    pub fn text(&self) -> String {
        let mut s = String::new();
        for (i, line) in self.lines.iter().enumerate() {
            if i > 0 {
                s.push('\n');
            }
            for c in line {
                s.push(*c);
            }
        }
        s
    }

    pub fn cursor(&self) -> (usize, usize) {
        self.cursor
    }

    pub fn lines(&self) -> &[Vec<char>] {
        &self.lines
    }

    pub fn insert(&mut self, c: char) {
        let (r, col) = self.cursor;
        if c == '\n' {
            let rest = self.lines[r].split_off(col);
            self.lines.insert(r + 1, rest);
            self.cursor = (r + 1, 0);
        } else {
            self.lines[r].insert(col, c);
            self.cursor.1 += 1;
        }
    }

    pub fn backspace(&mut self) {
        let (r, c) = self.cursor;
        if c > 0 {
            self.lines[r].remove(c - 1);
            self.cursor.1 -= 1;
        } else if r > 0 {
            let curr = self.lines.remove(r);
            let prev_len = self.lines[r - 1].len();
            self.lines[r - 1].extend(curr);
            self.cursor = (r - 1, prev_len);
        }
    }

    pub fn delete(&mut self) {
        let (r, c) = self.cursor;
        if c < self.lines[r].len() {
            self.lines[r].remove(c);
        } else if r + 1 < self.lines.len() {
            let next = self.lines.remove(r + 1);
            self.lines[r].extend(next);
        }
    }

    pub fn left(&mut self) {
        let (r, c) = self.cursor;
        if c > 0 {
            self.cursor.1 -= 1;
        } else if r > 0 {
            self.cursor = (r - 1, self.lines[r - 1].len());
        }
    }

    pub fn right(&mut self) {
        let (r, c) = self.cursor;
        if c < self.lines[r].len() {
            self.cursor.1 += 1;
        } else if r + 1 < self.lines.len() {
            self.cursor = (r + 1, 0);
        }
    }

    pub fn up(&mut self) {
        let (r, c) = self.cursor;
        if r > 0 {
            let new_r = r - 1;
            let new_c = c.min(self.lines[new_r].len());
            self.cursor = (new_r, new_c);
        }
    }

    pub fn down(&mut self) {
        let (r, c) = self.cursor;
        if r + 1 < self.lines.len() {
            let new_r = r + 1;
            let new_c = c.min(self.lines[new_r].len());
            self.cursor = (new_r, new_c);
        }
    }

    pub fn home(&mut self) {
        self.cursor.1 = 0;
    }

    pub fn end(&mut self) {
        let r = self.cursor.0;
        self.cursor.1 = self.lines[r].len();
    }

    pub fn word_left(&mut self) {
        let (r, mut c) = self.cursor;
        let line = &self.lines[r];
        while c > 0 && !is_word_char(line[c - 1]) {
            c -= 1;
        }
        while c > 0 && is_word_char(line[c - 1]) {
            c -= 1;
        }
        self.cursor.1 = c;
    }

    pub fn word_right(&mut self) {
        let (r, mut c) = self.cursor;
        let line = &self.lines[r];
        while c < line.len() && !is_word_char(line[c]) {
            c += 1;
        }
        while c < line.len() && is_word_char(line[c]) {
            c += 1;
        }
        self.cursor.1 = c;
    }

    pub fn delete_word_forward(&mut self) {
        let (r, start) = self.cursor;
        self.word_right();
        let end = self.cursor.1;
        self.lines[r].drain(start..end);
        self.cursor = (r, start);
    }

    pub fn delete_word_back(&mut self) {
        let (r, end) = self.cursor;
        self.word_left();
        let start = self.cursor.1;
        self.lines[r].drain(start..end);
    }
}
