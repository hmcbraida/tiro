use std::collections::HashSet;
use std::time::Instant;

use crate::note::StoredNote;

use super::input::{LineEditor, TextBuffer};

pub struct AppState {
    pub mode: Mode,
    pub error: Option<ErrorBox>,
    pub quit: bool,
    pub ctrl_x_pending: bool,
}

impl AppState {
    pub fn new(initial: SearchState) -> Self {
        Self {
            mode: Mode::Search(initial),
            error: None,
            quit: false,
            ctrl_x_pending: false,
        }
    }

    pub fn set_error(&mut self, msg: impl Into<String>) {
        self.error = Some(ErrorBox::new(msg.into()));
    }
}

pub enum Mode {
    Search(SearchState),
    NoteView(NoteViewState),
    TagPicker(TagPickerState),
}

pub struct SearchState {
    pub query: LineEditor,
    pub results: Vec<StoredNote>,
    pub cursor: usize,
    pub page: usize,
}

impl SearchState {
    pub fn new() -> Self {
        Self {
            query: LineEditor::new(),
            results: Vec::new(),
            cursor: 0,
            page: 0,
        }
    }
}

pub struct NoteViewState {
    pub note_id: String,
    pub buffer: TextBuffer,
    pub tags: HashSet<String>,
    pub prev_search: Box<SearchState>,
    pub dirty: bool,
    pub last_edit: Instant,
}

pub struct TagPickerState {
    pub origin: Box<NoteViewState>,
    pub filter: LineEditor,
    pub cursor: usize,
    pub pending: HashSet<String>,
}

pub struct ErrorBox {
    pub message: String,
    pub created_at: Instant,
}

impl ErrorBox {
    pub fn new(message: String) -> Self {
        Self {
            message,
            created_at: Instant::now(),
        }
    }
}
