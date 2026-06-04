use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use crate::agent::session::{AgentSession, TranscriptMessage};
use crate::agent::{AgentEvent, AgentRuntime, runtime::spawn_turn};
use crate::engine::{TiroEngine, generate_id};
use crate::filter::Filter;
use crate::note::Note;
use crate::store::NoteStore;

use super::action::{Action, EditOp};
use super::input::wrap;
use super::input::{LineEditor, TextBuffer};
use super::state::{
    AgentModalState, AppState, BaseMode, ErrorBox, InFlight, NoteViewState,
    Overlay, OverlayKind, SearchState, SessionPickerState, TagPickerState,
};

const IDLE_SAVE: Duration = Duration::from_secs(1);
const ERROR_LIFETIME: Duration = Duration::from_secs(5);

pub fn apply<S: NoteStore + Send + 'static>(
    state: &mut AppState,
    action: Action,
    engine: &Arc<Mutex<TiroEngine<S>>>,
    runtime: &AgentRuntime,
) {
    let was_ctrl_x = state.ctrl_x_pending;
    state.ctrl_x_pending = false;
    let _ = was_ctrl_x;

    match action {
        Action::Quit => {
            save_if_dirty(state, engine);
            cancel_in_flight(state);
            state.quit = true;
        }
        Action::StartCtrlX => {
            state.ctrl_x_pending = true;
        }
        Action::CancelCtrlX => {}
        Action::NewNote => {
            // If AgentModal is on top, Ctrl+X n means NewAgentSession.
            if matches!(
                state.top_overlay().map(|o| o.kind()),
                Some(OverlayKind::AgentModal)
            ) {
                apply(state, Action::NewAgentSession, engine, runtime);
                return;
            }
            new_note_flow(state, engine);
        }
        Action::ForceSave => {
            save_if_dirty(state, engine);
        }
        Action::OpenInEditor => {
            // intentional no-op; spec defers $EDITOR suspend flow.
        }
        Action::Edit(op) => apply_edit(state, op, engine),
        Action::ListDown => match top_focus(state) {
            Focus::SearchBase => {
                if let BaseMode::Search(s) = &mut state.base {
                    list_down_search(s, engine, &mut state.error);
                }
            }
            Focus::TagPicker => {
                if let Some(Overlay::TagPicker(tp)) = state.overlays.last_mut()
                {
                    let n = visible_tags_for(tp, engine).len();
                    if n > 0 && tp.cursor + 1 < n {
                        tp.cursor += 1;
                    }
                }
            }
            Focus::SessionPicker => {
                if let Some(Overlay::SessionPicker(sp)) =
                    state.overlays.last_mut()
                {
                    let n = visible_sessions(sp).len();
                    if n > 0 && sp.cursor + 1 < n {
                        sp.cursor += 1;
                    }
                }
            }
            _ => {}
        },
        Action::ListUp => match top_focus(state) {
            Focus::SearchBase => {
                if let BaseMode::Search(s) = &mut state.base {
                    list_up_search(s, engine, &mut state.error);
                }
            }
            Focus::TagPicker => {
                if let Some(Overlay::TagPicker(tp)) = state.overlays.last_mut()
                    && tp.cursor > 0
                {
                    tp.cursor -= 1;
                }
            }
            Focus::SessionPicker => {
                if let Some(Overlay::SessionPicker(sp)) =
                    state.overlays.last_mut()
                    && sp.cursor > 0
                {
                    sp.cursor -= 1;
                }
            }
            _ => {}
        },
        Action::Submit => match top_focus(state) {
            Focus::SearchBase => submit_search(state),
            Focus::NoteViewBase => {}
            Focus::TagPicker => {
                if let Some(Overlay::TagPicker(_)) = state.top_overlay() {
                    let Some(Overlay::TagPicker(tp)) = state.overlays.pop()
                    else {
                        return;
                    };
                    tag_picker_submit(state, tp, engine);
                }
            }
            Focus::AgentModal => {}
            Focus::SessionPicker => {}
        },
        Action::Cancel => match top_focus(state) {
            Focus::SearchBase => {
                if let BaseMode::Search(s) = &mut state.base {
                    s.query.clear();
                    refresh_search(s, engine, state_err_sink(&mut state.error));
                }
            }
            Focus::NoteViewBase => note_view_cancel(state, engine),
            Focus::TagPicker => {
                // discard pending; just pop.
                state.overlays.pop();
            }
            Focus::AgentModal => {}
            Focus::SessionPicker => {
                state.overlays.pop();
            }
        },
        Action::ToggleTagAt(idx) => {
            if let BaseMode::NoteView(nv) = &mut state.base {
                let names = sorted_tag_names(engine);
                if let Some(name) = names.get(idx).cloned() {
                    if nv.tags.contains(&name) {
                        nv.tags.remove(&name);
                    } else {
                        nv.tags.insert(name);
                    }
                    let tags: Vec<String> = nv.tags.iter().cloned().collect();
                    if let Err(e) = engine
                        .lock()
                        .expect("engine mutex")
                        .update_note_tags(&nv.note_id, tags)
                    {
                        state.set_error(format!("save failed: {e}"));
                    }
                }
            }
        }
        Action::OpenTagPicker => {
            if let BaseMode::NoteView(nv) = &state.base {
                let pending = nv.tags.clone();
                state.overlays.push(Overlay::TagPicker(TagPickerState {
                    filter: LineEditor::new(),
                    cursor: 0,
                    pending,
                }));
            }
        }
        Action::Toggle => {
            if let Some(Overlay::TagPicker(tp)) = state.overlays.last_mut() {
                let visible = visible_tags_for(tp, engine);
                if let Some(name) = visible.get(tp.cursor).cloned() {
                    if tp.pending.contains(&name) {
                        tp.pending.remove(&name);
                    } else {
                        tp.pending.insert(name);
                    }
                }
            }
        }
        Action::OpenAgentModal => open_agent_modal(state, engine, runtime),
        Action::SubmitAgentPrompt => {
            submit_agent_prompt(state, engine, runtime)
        }
        Action::AgentCancel => agent_cancel(state),
        Action::NewAgentSession => new_agent_session(state, runtime),
        Action::OpenSessionPicker => open_session_picker(state, runtime),
        Action::SubmitSessionPicker => {
            submit_session_picker(state, runtime);
        }
        Action::ScrollTranscriptUp => {
            if let Some(Overlay::AgentModal(m)) = state.overlays.last_mut() {
                m.scroll = m.scroll.saturating_sub(4);
            }
        }
        Action::ScrollTranscriptDown => {
            if let Some(Overlay::AgentModal(m)) = state.overlays.last_mut() {
                m.scroll = m.scroll.saturating_add(4);
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Focus {
    SearchBase,
    NoteViewBase,
    TagPicker,
    AgentModal,
    SessionPicker,
}

fn top_focus(state: &AppState) -> Focus {
    match state.top_overlay().map(|o| o.kind()) {
        Some(OverlayKind::TagPicker) => Focus::TagPicker,
        Some(OverlayKind::AgentModal) => Focus::AgentModal,
        Some(OverlayKind::SessionPicker) => Focus::SessionPicker,
        None => match state.base {
            BaseMode::Search(_) => Focus::SearchBase,
            BaseMode::NoteView(_) => Focus::NoteViewBase,
        },
    }
}

fn apply_edit<S: NoteStore + Send + 'static>(
    state: &mut AppState,
    op: EditOp,
    engine: &Arc<Mutex<TiroEngine<S>>>,
) {
    // Top overlay (if any) consumes edits; otherwise base.
    if let Some(top) = state.overlays.last_mut() {
        match top {
            Overlay::TagPicker(tp) => {
                apply_line(&mut tp.filter, op);
                tp.cursor = 0;
            }
            Overlay::AgentModal(m) => {
                if m.in_flight.is_none() {
                    apply_line(&mut m.input, op);
                }
            }
            Overlay::SessionPicker(sp) => {
                apply_line(&mut sp.filter, op);
                sp.cursor = 0;
            }
        }
        return;
    }
    match &mut state.base {
        BaseMode::Search(s) => {
            if apply_line(&mut s.query, op) {
                refresh_search(s, engine, state_err_sink(&mut state.error));
            }
        }
        BaseMode::NoteView(nv) => {
            match &op {
                EditOp::Up => {
                    apply_visual_vertical(nv, -1);
                }
                EditOp::Down => {
                    apply_visual_vertical(nv, 1);
                }
                _ => {
                    nv.desired_vcol = None;
                    apply_multiline(&mut nv.buffer, op);
                }
            }
            nv.dirty = true;
            nv.last_edit = Instant::now();
        }
    }
}

fn apply_visual_vertical(nv: &mut NoteViewState, delta: i32) {
    let width = nv.last_view_width as usize;
    if width == 0 {
        // No render yet -- fall back to buffer-line movement.
        if delta < 0 {
            nv.buffer.up();
        } else {
            nv.buffer.down();
        }
        return;
    }
    let lines = nv.buffer.lines();
    let (mapping_lines, mapping) = wrap::wrap_lines(lines, width);
    let _ = mapping_lines;
    let (row, col) = nv.buffer.cursor();
    let (cur_vrow, cur_vcol) =
        wrap::buffer_to_visual(&mapping, lines, row, col);

    let target_vcol = nv.desired_vcol.map(|v| v as usize).unwrap_or(cur_vcol);

    let target_vrow = if delta < 0 {
        if cur_vrow == 0 {
            // At top -- preserve buffer cursor; record desired column.
            nv.desired_vcol = Some(target_vcol as u16);
            return;
        }
        cur_vrow - 1
    } else {
        let last_vrow = mapping.total_visual_rows.saturating_sub(1);
        if cur_vrow >= last_vrow {
            nv.desired_vcol = Some(target_vcol as u16);
            return;
        }
        cur_vrow + 1
    };

    let (new_row, new_col) =
        wrap::visual_to_buffer(&mapping, lines, target_vrow, target_vcol);
    nv.buffer.set_cursor(new_row, new_col);
    nv.desired_vcol = Some(target_vcol as u16);
}

fn submit_search(state: &mut AppState) {
    let BaseMode::Search(s) = std::mem::replace(
        &mut state.base,
        BaseMode::Search(SearchState::new()),
    ) else {
        unreachable!();
    };
    if let Some(stored) = s.results.get(s.cursor).cloned() {
        let nv = NoteViewState {
            note_id: stored.id.clone(),
            buffer: TextBuffer::from_str(&stored.note.contents),
            tags: stored.note.tags.clone(),
            prev_search: Box::new(s),
            dirty: false,
            last_edit: Instant::now(),
            desired_vcol: None,
            scroll_offset: 0,
            last_view_width: 0,
        };
        state.base = BaseMode::NoteView(nv);
    } else {
        state.base = BaseMode::Search(s);
    }
}

fn note_view_cancel<S: NoteStore + Send + 'static>(
    state: &mut AppState,
    engine: &Arc<Mutex<TiroEngine<S>>>,
) {
    let BaseMode::NoteView(nv) = std::mem::replace(
        &mut state.base,
        BaseMode::Search(SearchState::new()),
    ) else {
        unreachable!();
    };
    let NoteViewState {
        note_id,
        buffer,
        prev_search,
        dirty,
        ..
    } = nv;
    if dirty
        && let Err(e) = engine
            .lock()
            .expect("engine mutex")
            .update_note_contents(&note_id, buffer.text())
    {
        state.set_error(format!("save failed: {e}"));
    }
    let mut prev = *prev_search;
    refresh_search(&mut prev, engine, state_err_sink(&mut state.error));
    state.base = BaseMode::Search(prev);
}

fn new_note_flow<S: NoteStore + Send + 'static>(
    state: &mut AppState,
    engine: &Arc<Mutex<TiroEngine<S>>>,
) {
    let prev = match std::mem::replace(
        &mut state.base,
        BaseMode::Search(SearchState::new()),
    ) {
        BaseMode::Search(s) => s,
        BaseMode::NoteView(nv) => {
            let id = nv.note_id.clone();
            let text = nv.buffer.text();
            if nv.dirty
                && let Err(e) = engine
                    .lock()
                    .expect("engine mutex")
                    .update_note_contents(&id, text)
            {
                state.set_error(format!("save failed: {e}"));
            }
            *nv.prev_search
        }
    };
    let created = engine
        .lock()
        .expect("engine mutex")
        .create_new_note(Note::new(String::new(), HashSet::new()));
    match created {
        Ok(stored) => {
            let nv = NoteViewState {
                note_id: stored.id.clone(),
                buffer: TextBuffer::new(),
                tags: HashSet::new(),
                prev_search: Box::new(prev),
                dirty: false,
                last_edit: Instant::now(),
                desired_vcol: None,
                scroll_offset: 0,
                last_view_width: 0,
            };
            state.base = BaseMode::NoteView(nv);
        }
        Err(e) => {
            state.set_error(format!("create failed: {e}"));
            state.base = BaseMode::Search(prev);
        }
    }
}

fn open_agent_modal<S: NoteStore + Send + 'static>(
    state: &mut AppState,
    _engine: &Arc<Mutex<TiroEngine<S>>>,
    runtime: &AgentRuntime,
) {
    if matches!(
        state.top_overlay().map(|o| o.kind()),
        Some(OverlayKind::AgentModal)
    ) {
        return;
    }
    let Some(agent) = runtime.enabled() else {
        // Disabled: still push a modal so the user sees the message.
        let session = AgentSession::new(generate_id());
        state
            .overlays
            .push(Overlay::AgentModal(AgentModalState::new(session)));
        return;
    };
    let mut session = AgentSession::new(generate_id());
    if let BaseMode::NoteView(nv) = &state.base {
        session.attached_note_ids.push(nv.note_id.clone());
    }
    if let Err(e) = agent.session_store.save(&session) {
        state.set_error(format!("session save: {e}"));
    }
    state
        .overlays
        .push(Overlay::AgentModal(AgentModalState::new(session)));
}

fn submit_agent_prompt<S: NoteStore + Send + 'static>(
    state: &mut AppState,
    engine: &Arc<Mutex<TiroEngine<S>>>,
    runtime: &AgentRuntime,
) {
    let Some(agent) = runtime.enabled() else {
        return;
    };
    let Some(Overlay::AgentModal(m)) = state.overlays.last_mut() else {
        return;
    };
    if m.in_flight.is_some() {
        return;
    }
    let prompt = m.input.text();
    if prompt.trim().is_empty() {
        return;
    }
    m.input.clear();

    let preamble = build_note_preamble(engine, &m.session.attached_note_ids);
    let user_full = match preamble {
        Some(p) if !p.is_empty() => format!("{p}\n\n{prompt}"),
        _ => prompt,
    };
    m.session
        .messages
        .push(TranscriptMessage::User(user_full.clone()));

    let (tx, rx) = mpsc::unbounded_channel();
    let session_for_task = m.session.clone();
    let (handle, _join) =
        spawn_turn(agent, engine.clone(), session_for_task, user_full, tx);
    m.in_flight = Some(InFlight {
        events: rx,
        cancel: Some(handle),
    });
    // The background task owns its own copy of the session and persists
    // it as the turn progresses; we mirror its events into `m.session`
    // here so the UI updates live, and reload the canonical state from
    // disk when the turn finishes (see `drain_agent_events`).
}

fn agent_cancel(state: &mut AppState) {
    let Some(Overlay::AgentModal(m)) = state.overlays.last_mut() else {
        return;
    };
    if let Some(in_flight) = m.in_flight.as_mut() {
        if let Some(handle) = in_flight.cancel.take() {
            let _ = handle.cancel.send(());
        }
        return;
    }
    // Idle Esc -> pop modal (and any session picker above, though by
    // construction we're already top).
    state.overlays.pop();
}

fn new_agent_session(state: &mut AppState, runtime: &AgentRuntime) {
    let Some(Overlay::AgentModal(m)) = state.overlays.last_mut() else {
        return;
    };
    let session = AgentSession::new(generate_id());
    if let Some(agent) = runtime.enabled() {
        let _ = agent.session_store.save(&session);
    }
    m.session = session;
    m.streaming_text.clear();
    m.scroll = 0;
    m.in_flight = None;
}

fn open_session_picker(state: &mut AppState, runtime: &AgentRuntime) {
    let Some(agent) = runtime.enabled() else {
        return;
    };
    let sessions = agent.session_store.list().unwrap_or_default();
    state
        .overlays
        .push(Overlay::SessionPicker(SessionPickerState::new(sessions)));
}

fn submit_session_picker(state: &mut AppState, runtime: &AgentRuntime) {
    let Some(Overlay::SessionPicker(_)) = state.top_overlay() else {
        return;
    };
    let Some(Overlay::SessionPicker(sp)) = state.overlays.pop() else {
        return;
    };
    let visible = visible_sessions(&sp);
    let Some(idx) = visible.get(sp.cursor).copied() else {
        return;
    };
    let Some(session) = sp.sessions.into_iter().nth(idx) else {
        return;
    };
    let session = if let Some(agent) = runtime.enabled() {
        agent.session_store.load(&session.id).unwrap_or(session)
    } else {
        session
    };
    if let Some(Overlay::AgentModal(m)) = state.overlays.last_mut() {
        m.session = session;
        m.streaming_text.clear();
        m.scroll = 0;
        m.in_flight = None;
    }
}

fn build_note_preamble<S: NoteStore>(
    engine: &Arc<Mutex<TiroEngine<S>>>,
    ids: &[String],
) -> Option<String> {
    if ids.is_empty() {
        return None;
    }
    let eng = engine.lock().expect("engine mutex");
    let mut out = String::from("[attached notes]\n");
    for id in ids {
        if let Ok(n) = eng.get_note(id) {
            let mut tags: Vec<&str> =
                n.note.tags.iter().map(String::as_str).collect();
            tags.sort();
            out.push_str(&format!(
                "- id: {} tags: {}\n  ---\n  {}\n",
                n.id,
                tags.join(","),
                n.note.contents
            ));
        }
    }
    Some(out)
}

fn cancel_in_flight(state: &mut AppState) {
    for ov in state.overlays.iter_mut() {
        if let Overlay::AgentModal(m) = ov
            && let Some(in_flight) = m.in_flight.as_mut()
            && let Some(handle) = in_flight.cancel.take()
        {
            let _ = handle.cancel.send(());
        }
    }
}

pub fn tick<S: NoteStore + Send + 'static>(
    state: &mut AppState,
    engine: &Arc<Mutex<TiroEngine<S>>>,
    runtime: &AgentRuntime,
) {
    if let Some(err) = &state.error
        && err.created_at.elapsed() > ERROR_LIFETIME
    {
        state.error = None;
    }
    if let BaseMode::NoteView(nv) = &mut state.base
        && nv.dirty
        && nv.last_edit.elapsed() >= IDLE_SAVE
    {
        let id = nv.note_id.clone();
        let text = nv.buffer.text();
        let res = engine
            .lock()
            .expect("engine mutex")
            .update_note_contents(&id, text);
        match res {
            Ok(()) => nv.dirty = false,
            Err(e) => {
                let msg = format!("save failed: {e}");
                state.set_error(msg);
            }
        }
    }
    drain_agent_events(state, runtime);
}

fn drain_agent_events(state: &mut AppState, runtime: &AgentRuntime) {
    let mut session_id_to_reload: Option<String> = None;
    if let Some(Overlay::AgentModal(m)) = state.overlays.last_mut()
        && let Some(in_flight) = m.in_flight.as_mut()
    {
        loop {
            match in_flight.events.try_recv() {
                Ok(AgentEvent::Token(text)) => {
                    m.streaming_text.push_str(&text);
                }
                Ok(AgentEvent::AssistantMessageComplete) => {
                    let body = std::mem::take(&mut m.streaming_text);
                    if !body.is_empty() {
                        m.session
                            .messages
                            .push(TranscriptMessage::Assistant(body));
                    }
                }
                Ok(AgentEvent::ToolCall { name, args }) => {
                    m.session
                        .messages
                        .push(TranscriptMessage::ToolCall { name, args });
                }
                Ok(AgentEvent::ToolResult {
                    name,
                    result,
                    is_error,
                }) => {
                    m.session.messages.push(TranscriptMessage::ToolResult {
                        name,
                        result,
                        is_error,
                    });
                }
                Ok(AgentEvent::Error(msg)) => {
                    m.session
                        .messages
                        .push(TranscriptMessage::SystemError(msg));
                }
                Ok(AgentEvent::Done) => {
                    m.in_flight = None;
                    session_id_to_reload = Some(m.session.id.clone());
                    break;
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    m.in_flight = None;
                    break;
                }
            }
        }
    }
    // Re-load the canonical session from disk after a turn completes so
    // we pick up the user-message preamble + persisted state in case of
    // any divergence with our local mirror.
    if let Some(id) = session_id_to_reload
        && let Some(agent) = runtime.enabled()
        && let Ok(loaded) = agent.session_store.load(&id)
        && let Some(Overlay::AgentModal(m)) = state.overlays.last_mut()
    {
        m.session = loaded;
    }
}

fn state_err_sink(err: &mut Option<ErrorBox>) -> impl FnMut(String) + '_ {
    move |msg: String| {
        *err = Some(ErrorBox::new(msg));
    }
}

fn refresh_search<S: NoteStore>(
    s: &mut SearchState,
    engine: &Arc<Mutex<TiroEngine<S>>>,
    mut on_err: impl FnMut(String),
) {
    s.page = 0;
    s.cursor = 0;
    let filter = Filter::parse(&s.query.text());
    let res = engine
        .lock()
        .expect("engine mutex")
        .get_notes_page(&filter, 0);
    match res {
        Ok(rows) => s.results = rows,
        Err(e) => {
            s.results.clear();
            on_err(format!("load failed: {e}"));
        }
    }
}

fn list_down_search<S: NoteStore>(
    s: &mut SearchState,
    engine: &Arc<Mutex<TiroEngine<S>>>,
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
    let filter = Filter::parse(&s.query.text());
    let res = engine
        .lock()
        .expect("engine mutex")
        .get_notes_page(&filter, next_page);
    match res {
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
    engine: &Arc<Mutex<TiroEngine<S>>>,
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
    let filter = Filter::parse(&s.query.text());
    let res = engine
        .lock()
        .expect("engine mutex")
        .get_notes_page(&filter, prev_page);
    match res {
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
    engine: &Arc<Mutex<TiroEngine<S>>>,
) {
    if let BaseMode::NoteView(nv) = &mut state.base
        && nv.dirty
    {
        let id = nv.note_id.clone();
        let text = nv.buffer.text();
        let res = engine
            .lock()
            .expect("engine mutex")
            .update_note_contents(&id, text);
        match res {
            Ok(()) => nv.dirty = false,
            Err(e) => state.set_error(format!("save failed: {e}")),
        }
    }
}

fn tag_picker_submit<S: NoteStore>(
    state: &mut AppState,
    mut tp: TagPickerState,
    engine: &Arc<Mutex<TiroEngine<S>>>,
) {
    let filter = tp.filter.text();
    if !filter.is_empty() {
        let visible = visible_tags_named(engine, &filter);
        if visible.is_empty() {
            let tag = crate::note::Tag::new(filter.clone(), None);
            engine.lock().expect("engine mutex").register_tag(tag);
            tp.pending.insert(filter);
        }
    }

    if let BaseMode::NoteView(nv) = &mut state.base {
        nv.tags = tp.pending.clone();
        let tags: Vec<String> = nv.tags.iter().cloned().collect();
        if let Err(e) = engine
            .lock()
            .expect("engine mutex")
            .update_note_tags(&nv.note_id, tags)
        {
            state.set_error(format!("save failed: {e}"));
        }
    }
}

fn sorted_tag_names<S: NoteStore>(
    engine: &Arc<Mutex<TiroEngine<S>>>,
) -> Vec<String> {
    let eng = engine.lock().expect("engine mutex");
    let mut names: Vec<String> =
        eng.get_tags().map(|t| t.name.clone()).collect();
    names.sort();
    names
}

pub fn visible_tags_for<S: NoteStore>(
    tp: &TagPickerState,
    engine: &Arc<Mutex<TiroEngine<S>>>,
) -> Vec<String> {
    visible_tags_named(engine, &tp.filter.text())
}

fn visible_tags_named<S: NoteStore>(
    engine: &Arc<Mutex<TiroEngine<S>>>,
    filter: &str,
) -> Vec<String> {
    let eng = engine.lock().expect("engine mutex");
    let mut names: Vec<String> = eng
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

pub fn visible_sessions(sp: &SessionPickerState) -> Vec<usize> {
    let q = sp.filter.text().to_lowercase();
    sp.sessions
        .iter()
        .enumerate()
        .filter(|(_, s)| q.is_empty() || s.title().to_lowercase().contains(&q))
        .map(|(i, _)| i)
        .collect()
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
