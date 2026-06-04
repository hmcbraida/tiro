pub mod agent_modal;
pub mod error_box;
pub mod note_view;
pub mod search;
pub mod session_picker;
pub mod tag_picker;
pub mod widgets;

use std::sync::{Arc, Mutex};

use ratatui::Frame;

use crate::engine::TiroEngine;
use crate::store::NoteStore;

use super::state::{AppState, BaseMode, Overlay};

pub fn dispatch<S: NoteStore>(
    state: &mut AppState,
    engine: &Arc<Mutex<TiroEngine<S>>>,
    frame: &mut Frame,
) {
    let area = frame.area();
    match &mut state.base {
        BaseMode::Search(s) => search::render(frame, area, s, engine),
        BaseMode::NoteView(nv) => note_view::render(frame, area, nv, engine),
    }
    for overlay in &state.overlays {
        match overlay {
            Overlay::TagPicker(tp) => {
                tag_picker::render(frame, area, tp, engine)
            }
            Overlay::AgentModal(m) => agent_modal::render(frame, area, m),
            Overlay::SessionPicker(sp) => {
                session_picker::render(frame, area, sp)
            }
        }
    }
    if let Some(err) = &state.error {
        error_box::render(frame, area, err);
    }
}
