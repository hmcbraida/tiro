use std::collections::HashSet;
use std::time::Instant;

use tokio::sync::mpsc;

use crate::agent::session::AgentSession;
use crate::agent::{AgentEvent, runtime::TurnHandle};
use crate::note::StoredNote;

use super::input::{LineEditor, TextBuffer};

pub struct AppState {
    pub base: BaseMode,
    pub overlays: Vec<Overlay>,
    pub error: Option<ErrorBox>,
    pub quit: bool,
    pub ctrl_x_pending: bool,
}

impl AppState {
    pub fn new(initial: SearchState) -> Self {
        Self {
            base: BaseMode::Search(initial),
            overlays: Vec::new(),
            error: None,
            quit: false,
            ctrl_x_pending: false,
        }
    }

    pub fn set_error(&mut self, msg: impl Into<String>) {
        self.error = Some(ErrorBox::new(msg.into()));
    }

    pub fn top_overlay(&self) -> Option<&Overlay> {
        self.overlays.last()
    }

    #[allow(dead_code)]
    pub fn top_overlay_mut(&mut self) -> Option<&mut Overlay> {
        self.overlays.last_mut()
    }
}

pub enum BaseMode {
    Search(SearchState),
    NoteView(NoteViewState),
}

impl BaseMode {
    pub fn kind(&self) -> BaseModeKind {
        match self {
            BaseMode::Search(_) => BaseModeKind::Search,
            BaseMode::NoteView(_) => BaseModeKind::NoteView,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseModeKind {
    Search,
    NoteView,
}

pub enum Overlay {
    TagPicker(TagPickerState),
    AgentModal(AgentModalState),
    SessionPicker(SessionPickerState),
}

impl Overlay {
    pub fn kind(&self) -> OverlayKind {
        match self {
            Overlay::TagPicker(_) => OverlayKind::TagPicker,
            Overlay::AgentModal(_) => OverlayKind::AgentModal,
            Overlay::SessionPicker(_) => OverlayKind::SessionPicker,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayKind {
    TagPicker,
    AgentModal,
    SessionPicker,
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
    /// Sticky desired visual column for vertical movement through wrapped /
    /// short lines. Set on the first up/down, cleared on any horizontal or
    /// editing op.
    pub desired_vcol: Option<u16>,
    /// Top visible visual row of the body. Adjusted at render time to keep
    /// the cursor on-screen.
    pub scroll_offset: u16,
    /// Inner body width recorded on the last render, used by update.rs to
    /// compute visual up/down before the next render. Zero before any render.
    pub last_view_width: u16,
}

pub struct TagPickerState {
    pub filter: LineEditor,
    pub cursor: usize,
    pub pending: HashSet<String>,
}

pub struct AgentModalState {
    pub session: AgentSession,
    pub input: LineEditor,
    pub scroll: u16,
    /// Maximum scroll offset computed by the last render (so update handlers
    /// can clamp on key/mouse events without re-wrapping). Zero before any
    /// render or when content fits in the viewport.
    pub last_max_scroll: u16,
    /// When true, the renderer pins `scroll` to `last_max_scroll` so new
    /// content keeps the latest line in view. Cleared when the user
    /// scrolls up; re-set when they scroll back to the bottom or submit
    /// a new prompt.
    pub follow_tail: bool,
    /// Set while a turn is streaming. When `Some`, the input area is
    /// replaced with a spinner + cancel hint.
    pub in_flight: Option<InFlight>,
    /// Tokens accumulated for the assistant message being streamed right
    /// now (drained into `session.messages` on AssistantMessageComplete).
    pub streaming_text: String,
}

pub struct InFlight {
    pub events: mpsc::UnboundedReceiver<AgentEvent>,
    pub cancel: Option<TurnHandle>,
}

impl AgentModalState {
    pub fn new(session: AgentSession) -> Self {
        Self {
            session,
            input: LineEditor::new(),
            scroll: 0,
            last_max_scroll: 0,
            follow_tail: true,
            in_flight: None,
            streaming_text: String::new(),
        }
    }
}

pub struct SessionPickerState {
    pub filter: LineEditor,
    pub cursor: usize,
    pub sessions: Vec<AgentSession>,
}

impl SessionPickerState {
    pub fn new(sessions: Vec<AgentSession>) -> Self {
        Self {
            filter: LineEditor::new(),
            cursor: 0,
            sessions,
        }
    }
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
