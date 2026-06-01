pub mod error_box;
pub mod note_view;
pub mod search;
pub mod tag_picker;
pub mod widgets;

use ratatui::Frame;

use crate::engine::TiroEngine;
use crate::store::NoteStore;

use super::state::{AppState, Mode};

pub fn dispatch<S: NoteStore>(
    state: &AppState,
    engine: &TiroEngine<S>,
    frame: &mut Frame,
) {
    let area = frame.area();
    match &state.mode {
        Mode::Search(s) => search::render(frame, area, s, engine),
        Mode::NoteView(nv) => note_view::render(frame, area, nv, engine),
        Mode::TagPicker(tp) => tag_picker::render(frame, area, tp, engine),
    }
    if let Some(err) = &state.error {
        error_box::render(frame, area, err);
    }
}
