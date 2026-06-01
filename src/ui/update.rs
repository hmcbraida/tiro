use std::collections::HashSet;
use std::time::{Duration, Instant};

use crate::engine::TiroEngine;
use crate::note::Note;
use crate::store::NoteStore;

use super::action::{Action, EditOp};
use super::input::{LineEditor, TextBuffer};
use super::state::{
    AppState, ErrorBox, Mode, NoteViewState, SearchState, TagPickerState,
};

const IDLE_SAVE: Duration = Duration::from_secs(1);
const ERROR_LIFETIME: Duration = Duration::from_secs(5);

pub fn apply<S: NoteStore>(
    state: &mut AppState,
    action: Action,
    engine: &mut TiroEngine<S>,
) {
    let was_ctrl_x = state.ctrl_x_pending;
    state.ctrl_x_pending = false;

    match action {
        Action::Quit => {
            save_if_dirty(state, engine);
            state.quit = true;
        }
        Action::StartCtrlX => {
            state.ctrl_x_pending = true;
        }
        Action::CancelCtrlX => {}
        Action::NewNote => {
            let _ = was_ctrl_x;
            let prev = match std::mem::replace(
                &mut state.mode,
                Mode::Search(SearchState::new()),
            ) {
                Mode::Search(s) => s,
                Mode::NoteView(nv) => {
                    let id = nv.note_id.clone();
                    let text = nv.buffer.text();
                    if nv.dirty
                        && let Err(e) = engine.update_note_contents(&id, text)
                    {
                        state.set_error(format!("save failed: {e}"));
                    }
                    *nv.prev_search
                }
                Mode::TagPicker(tp) => *tp.origin.prev_search,
            };
            match engine
                .create_new_note(Note::new(String::new(), HashSet::new()))
            {
                Ok(stored) => {
                    let nv = NoteViewState {
                        note_id: stored.id.clone(),
                        buffer: TextBuffer::new(),
                        tags: HashSet::new(),
                        prev_search: Box::new(prev),
                        dirty: false,
                        last_edit: Instant::now(),
                    };
                    state.mode = Mode::NoteView(nv);
                }
                Err(e) => {
                    state.set_error(format!("create failed: {e}"));
                    state.mode = Mode::Search(prev);
                }
            }
        }
        Action::ForceSave => {
            save_if_dirty(state, engine);
        }
        Action::OpenInEditor => {
            // intentional no-op for v0.1 -- spec lists it but the suspended
            // $EDITOR flow is large enough to defer.
        }
        Action::Edit(op) => match &mut state.mode {
            Mode::Search(s) => {
                if apply_line(&mut s.query, op) {
                    refresh_search(s, engine, state_err_sink(&mut state.error));
                }
            }
            Mode::NoteView(nv) => {
                apply_multiline(&mut nv.buffer, op);
                nv.dirty = true;
                nv.last_edit = Instant::now();
            }
            Mode::TagPicker(tp) => {
                apply_line(&mut tp.filter, op);
                tp.cursor = 0;
            }
        },
        Action::ListDown => match &mut state.mode {
            Mode::Search(s) => list_down_search(s, engine, &mut state.error),
            Mode::TagPicker(tp) => {
                let n = visible_tags(tp, engine).len();
                if n > 0 && tp.cursor + 1 < n {
                    tp.cursor += 1;
                }
            }
            _ => {}
        },
        Action::ListUp => match &mut state.mode {
            Mode::Search(s) => list_up_search(s, engine, &mut state.error),
            Mode::TagPicker(tp) => {
                if tp.cursor > 0 {
                    tp.cursor -= 1;
                }
            }
            _ => {}
        },
        Action::Submit => match std::mem::replace(
            &mut state.mode,
            Mode::Search(SearchState::new()),
        ) {
            Mode::Search(s) => {
                if let Some(stored) = s.results.get(s.cursor).cloned() {
                    let nv = NoteViewState {
                        note_id: stored.id.clone(),
                        buffer: TextBuffer::from_str(&stored.note.contents),
                        tags: stored.note.tags.clone(),
                        prev_search: Box::new(s),
                        dirty: false,
                        last_edit: Instant::now(),
                    };
                    state.mode = Mode::NoteView(nv);
                } else {
                    // restore as-is
                    state.mode = Mode::Search(s);
                }
            }
            Mode::NoteView(nv) => {
                state.mode = Mode::NoteView(nv);
            }
            Mode::TagPicker(tp) => {
                tag_picker_submit(state, tp, engine);
            }
        },
        Action::Cancel => match std::mem::replace(
            &mut state.mode,
            Mode::Search(SearchState::new()),
        ) {
            Mode::Search(mut s) => {
                s.query.clear();
                refresh_search(
                    &mut s,
                    engine,
                    state_err_sink(&mut state.error),
                );
                state.mode = Mode::Search(s);
            }
            Mode::NoteView(nv) => {
                let NoteViewState {
                    note_id,
                    buffer,
                    prev_search,
                    dirty,
                    ..
                } = nv;
                if dirty
                    && let Err(e) =
                        engine.update_note_contents(&note_id, buffer.text())
                {
                    state.set_error(format!("save failed: {e}"));
                }
                let mut prev = *prev_search;
                refresh_search(
                    &mut prev,
                    engine,
                    state_err_sink(&mut state.error),
                );
                state.mode = Mode::Search(prev);
            }
            Mode::TagPicker(tp) => {
                // discard pending; return to note view
                state.mode = Mode::NoteView(*tp.origin);
            }
        },
        Action::ToggleTagAt(idx) => {
            if let Mode::NoteView(nv) = &mut state.mode {
                let names = sorted_tag_names(engine);
                if let Some(name) = names.get(idx).cloned() {
                    if nv.tags.contains(&name) {
                        nv.tags.remove(&name);
                    } else {
                        nv.tags.insert(name);
                    }
                    let tags: Vec<String> = nv.tags.iter().cloned().collect();
                    if let Err(e) = engine.update_note_tags(&nv.note_id, tags) {
                        state.set_error(format!("save failed: {e}"));
                    }
                }
            }
        }
        Action::OpenTagPicker => {
            if let Mode::NoteView(_) = state.mode {
                let mode = std::mem::replace(
                    &mut state.mode,
                    Mode::Search(SearchState::new()),
                );
                if let Mode::NoteView(nv) = mode {
                    let pending = nv.tags.clone();
                    state.mode = Mode::TagPicker(TagPickerState {
                        origin: Box::new(nv),
                        filter: LineEditor::new(),
                        cursor: 0,
                        pending,
                    });
                }
            }
        }
        Action::Toggle => {
            if let Mode::TagPicker(tp) = &mut state.mode {
                let visible = visible_tags(tp, engine);
                if let Some(name) = visible.get(tp.cursor).cloned() {
                    if tp.pending.contains(&name) {
                        tp.pending.remove(&name);
                    } else {
                        tp.pending.insert(name);
                    }
                }
            }
        }
    }
}

pub fn tick<S: NoteStore>(state: &mut AppState, engine: &mut TiroEngine<S>) {
    if let Some(err) = &state.error
        && err.created_at.elapsed() > ERROR_LIFETIME
    {
        state.error = None;
    }
    if let Mode::NoteView(nv) = &mut state.mode
        && nv.dirty
        && nv.last_edit.elapsed() >= IDLE_SAVE
    {
        let id = nv.note_id.clone();
        let text = nv.buffer.text();
        match engine.update_note_contents(&id, text) {
            Ok(()) => nv.dirty = false,
            Err(e) => {
                let msg = format!("save failed: {e}");
                state.set_error(msg);
            }
        }
    }
}

fn state_err_sink(err: &mut Option<ErrorBox>) -> impl FnMut(String) + '_ {
    move |msg: String| {
        *err = Some(ErrorBox::new(msg));
    }
}

fn refresh_search<S: NoteStore>(
    s: &mut SearchState,
    engine: &TiroEngine<S>,
    mut on_err: impl FnMut(String),
) {
    s.page = 0;
    s.cursor = 0;
    match engine.get_notes_page(&s.query.text(), 0) {
        Ok(rows) => s.results = rows,
        Err(e) => {
            s.results.clear();
            on_err(format!("load failed: {e}"));
        }
    }
}

fn list_down_search<S: NoteStore>(
    s: &mut SearchState,
    engine: &TiroEngine<S>,
    err: &mut Option<ErrorBox>,
) {
    if s.results.is_empty() {
        return;
    }
    if s.cursor + 1 < s.results.len() {
        s.cursor += 1;
        return;
    }
    if s.results.len() < 20 {
        return;
    }
    let next_page = s.page + 1;
    match engine.get_notes_page(&s.query.text(), next_page) {
        Ok(rows) if !rows.is_empty() => {
            s.results = rows;
            s.page = next_page;
            s.cursor = 0;
        }
        Ok(_) => {}
        Err(e) => *err = Some(ErrorBox::new(format!("load failed: {e}"))),
    }
}

fn list_up_search<S: NoteStore>(
    s: &mut SearchState,
    engine: &TiroEngine<S>,
    err: &mut Option<ErrorBox>,
) {
    if s.cursor > 0 {
        s.cursor -= 1;
        return;
    }
    if s.page == 0 {
        return;
    }
    let prev_page = s.page - 1;
    match engine.get_notes_page(&s.query.text(), prev_page) {
        Ok(rows) if !rows.is_empty() => {
            s.cursor = rows.len() - 1;
            s.results = rows;
            s.page = prev_page;
        }
        Ok(_) => {}
        Err(e) => *err = Some(ErrorBox::new(format!("load failed: {e}"))),
    }
}

fn save_if_dirty<S: NoteStore>(
    state: &mut AppState,
    engine: &mut TiroEngine<S>,
) {
    if let Mode::NoteView(nv) = &mut state.mode
        && nv.dirty
    {
        let id = nv.note_id.clone();
        let text = nv.buffer.text();
        match engine.update_note_contents(&id, text) {
            Ok(()) => nv.dirty = false,
            Err(e) => state.set_error(format!("save failed: {e}")),
        }
    }
}

fn tag_picker_submit<S: NoteStore>(
    state: &mut AppState,
    mut tp: TagPickerState,
    engine: &mut TiroEngine<S>,
) {
    // If filter is non-empty and no tag matches, create a new one
    let filter = tp.filter.text();
    if !filter.is_empty() {
        let visible = visible_tags_named(engine, &filter);
        if visible.is_empty() {
            let tag = crate::note::Tag::new(filter.clone());
            engine.register_tag(tag);
            tp.pending.insert(filter);
        }
    }

    let mut nv = *tp.origin;
    nv.tags = tp.pending.clone();
    let tags: Vec<String> = nv.tags.iter().cloned().collect();
    if let Err(e) = engine.update_note_tags(&nv.note_id, tags) {
        state.set_error(format!("save failed: {e}"));
    }
    state.mode = Mode::NoteView(nv);
}

fn sorted_tag_names<S: NoteStore>(engine: &TiroEngine<S>) -> Vec<String> {
    let mut names: Vec<String> =
        engine.get_tags().map(|t| t.name.clone()).collect();
    names.sort();
    names
}

pub fn visible_tags<S: NoteStore>(
    tp: &TagPickerState,
    engine: &TiroEngine<S>,
) -> Vec<String> {
    visible_tags_named(engine, &tp.filter.text())
}

fn visible_tags_named<S: NoteStore>(
    engine: &TiroEngine<S>,
    filter: &str,
) -> Vec<String> {
    let mut names: Vec<String> = engine
        .get_tags()
        .map(|t| t.name.clone())
        .filter(|n| {
            filter.is_empty()
                || n.to_lowercase().contains(&filter.to_lowercase())
        })
        .collect();
    names.sort();
    names
}

fn apply_line(editor: &mut LineEditor, op: EditOp) -> bool {
    match op {
        EditOp::Insert(c) => {
            editor.insert(c);
            true
        }
        EditOp::Backspace => {
            editor.backspace();
            true
        }
        EditOp::Delete => {
            editor.delete();
            true
        }
        EditOp::Left => {
            editor.left();
            false
        }
        EditOp::Right => {
            editor.right();
            false
        }
        EditOp::Up | EditOp::Down => false,
        EditOp::WordLeft => {
            editor.word_left();
            false
        }
        EditOp::WordRight => {
            editor.word_right();
            false
        }
        EditOp::Home => {
            editor.home();
            false
        }
        EditOp::End => {
            editor.end();
            false
        }
        EditOp::DeleteWordForward => {
            editor.delete_word_forward();
            true
        }
        EditOp::DeleteWordBack => {
            editor.delete_word_back();
            true
        }
        EditOp::Newline => false,
    }
}

fn apply_multiline(buf: &mut TextBuffer, op: EditOp) {
    match op {
        EditOp::Insert(c) => buf.insert(c),
        EditOp::Newline => buf.insert('\n'),
        EditOp::Backspace => buf.backspace(),
        EditOp::Delete => buf.delete(),
        EditOp::Left => buf.left(),
        EditOp::Right => buf.right(),
        EditOp::Up => buf.up(),
        EditOp::Down => buf.down(),
        EditOp::WordLeft => buf.word_left(),
        EditOp::WordRight => buf.word_right(),
        EditOp::Home => buf.home(),
        EditOp::End => buf.end(),
        EditOp::DeleteWordForward => buf.delete_word_forward(),
        EditOp::DeleteWordBack => buf.delete_word_back(),
    }
}
