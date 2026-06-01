//! Context-aware key bindings.
//!
//! The keymap is a flat ordered list of bindings; the first one whose
//! context matches the current [`AppState`] wins. Precedence is encoded
//! as binding order -- see [`Keymap::default`].

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::action::{Action, EditOp};
use super::state::{AppState, BaseModeKind, OverlayKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeySpec {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}

impl KeySpec {
    pub const fn new(code: KeyCode, mods: KeyModifiers) -> Self {
        Self { code, mods }
    }

    fn matches(&self, ev: &KeyEvent) -> bool {
        self.code == ev.code && self.mods == ev.modifiers
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMatcher {
    /// Always applies (e.g. Quit).
    Any,
    /// Applies when no overlay is active and the base matches.
    Base(BaseModeKind),
    /// Applies when the top overlay is of this kind.
    Overlay(OverlayKind),
    /// Applies while Ctrl+X is pending -- orthogonal to base/overlay.
    CtrlXPending,
    /// Applies when no overlay is active (any base). Reserved for future
    /// bindings that should fire only outside of overlays.
    #[allow(dead_code)]
    AnyBase,
}

impl ContextMatcher {
    fn matches(&self, state: &AppState) -> bool {
        match self {
            ContextMatcher::Any => true,
            ContextMatcher::CtrlXPending => state.ctrl_x_pending,
            ContextMatcher::AnyBase => state.overlays.is_empty(),
            ContextMatcher::Base(kind) => {
                state.overlays.is_empty() && state.base.kind() == *kind
            }
            ContextMatcher::Overlay(kind) => match state.top_overlay() {
                Some(o) => o.kind() == *kind,
                None => false,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct Binding {
    pub key: KeySpec,
    pub context: ContextMatcher,
    pub action: Action,
}

pub struct Keymap {
    bindings: Vec<Binding>,
}

impl Keymap {
    pub fn translate(&self, ev: KeyEvent, state: &AppState) -> Option<Action> {
        for b in &self.bindings {
            if b.context.matches(state) && b.key.matches(&ev) {
                return Some(b.action.clone());
            }
        }
        // Ctrl+X pending: any unmatched key cancels.
        if state.ctrl_x_pending {
            return Some(Action::CancelCtrlX);
        }
        // Edit-line fallthrough for whichever input is in focus.
        let edit = if let Some(OverlayKind::AgentModal) =
            state.top_overlay().map(|o| o.kind())
        {
            translate_edit_line(&ev)
        } else if state.overlays.is_empty()
            && matches!(state.base.kind(), BaseModeKind::NoteView)
        {
            translate_edit_multiline(&ev)
        } else {
            translate_edit_line(&ev)
        };
        edit.map(Action::Edit)
    }
}

impl Default for Keymap {
    fn default() -> Self {
        let bindings = vec![
            // ---- global ----
            Binding {
                key: KeySpec::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                context: ContextMatcher::Any,
                action: Action::Quit,
            },
            // ---- Ctrl+X pending (must come before Ctrl+X start) ----
            Binding {
                key: KeySpec::new(KeyCode::Char('n'), KeyModifiers::NONE),
                context: ContextMatcher::CtrlXPending,
                // The handler dispatches NewAgentSession vs NewNote based
                // on whether the AgentModal overlay is active. We use
                // NewNote here as the base intent; update.rs branches.
                action: Action::NewNote,
            },
            Binding {
                key: KeySpec::new(KeyCode::Char('e'), KeyModifiers::NONE),
                context: ContextMatcher::CtrlXPending,
                action: Action::OpenInEditor,
            },
            // ---- Ctrl+X start ----
            Binding {
                key: KeySpec::new(KeyCode::Char('x'), KeyModifiers::CONTROL),
                context: ContextMatcher::Any,
                action: Action::StartCtrlX,
            },
            // ---- Ctrl+? -> Open agent modal ----
            Binding {
                key: KeySpec::new(KeyCode::Char('/'), KeyModifiers::CONTROL),
                context: ContextMatcher::Any,
                action: Action::OpenAgentModal,
            },
            // ---- AgentModal overlay ----
            Binding {
                key: KeySpec::new(KeyCode::Enter, KeyModifiers::NONE),
                context: ContextMatcher::Overlay(OverlayKind::AgentModal),
                action: Action::SubmitAgentPrompt,
            },
            Binding {
                key: KeySpec::new(KeyCode::Esc, KeyModifiers::NONE),
                context: ContextMatcher::Overlay(OverlayKind::AgentModal),
                action: Action::AgentCancel,
            },
            Binding {
                key: KeySpec::new(KeyCode::Char(';'), KeyModifiers::ALT),
                context: ContextMatcher::Overlay(OverlayKind::AgentModal),
                action: Action::OpenSessionPicker,
            },
            Binding {
                key: KeySpec::new(KeyCode::PageUp, KeyModifiers::NONE),
                context: ContextMatcher::Overlay(OverlayKind::AgentModal),
                action: Action::ScrollTranscriptUp,
            },
            Binding {
                key: KeySpec::new(KeyCode::PageDown, KeyModifiers::NONE),
                context: ContextMatcher::Overlay(OverlayKind::AgentModal),
                action: Action::ScrollTranscriptDown,
            },
            Binding {
                key: KeySpec::new(KeyCode::Char('v'), KeyModifiers::CONTROL),
                context: ContextMatcher::Overlay(OverlayKind::AgentModal),
                action: Action::ScrollTranscriptDown,
            },
            Binding {
                key: KeySpec::new(KeyCode::Char('v'), KeyModifiers::ALT),
                context: ContextMatcher::Overlay(OverlayKind::AgentModal),
                action: Action::ScrollTranscriptUp,
            },
            // ---- SessionPicker overlay ----
            Binding {
                key: KeySpec::new(KeyCode::Enter, KeyModifiers::NONE),
                context: ContextMatcher::Overlay(OverlayKind::SessionPicker),
                action: Action::SubmitSessionPicker,
            },
            Binding {
                key: KeySpec::new(KeyCode::Esc, KeyModifiers::NONE),
                context: ContextMatcher::Overlay(OverlayKind::SessionPicker),
                action: Action::Cancel,
            },
            Binding {
                key: KeySpec::new(KeyCode::Down, KeyModifiers::NONE),
                context: ContextMatcher::Overlay(OverlayKind::SessionPicker),
                action: Action::ListDown,
            },
            Binding {
                key: KeySpec::new(KeyCode::Up, KeyModifiers::NONE),
                context: ContextMatcher::Overlay(OverlayKind::SessionPicker),
                action: Action::ListUp,
            },
            Binding {
                key: KeySpec::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
                context: ContextMatcher::Overlay(OverlayKind::SessionPicker),
                action: Action::ListDown,
            },
            Binding {
                key: KeySpec::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
                context: ContextMatcher::Overlay(OverlayKind::SessionPicker),
                action: Action::ListUp,
            },
            // ---- TagPicker overlay ----
            Binding {
                key: KeySpec::new(KeyCode::Enter, KeyModifiers::NONE),
                context: ContextMatcher::Overlay(OverlayKind::TagPicker),
                action: Action::Submit,
            },
            Binding {
                key: KeySpec::new(KeyCode::Esc, KeyModifiers::NONE),
                context: ContextMatcher::Overlay(OverlayKind::TagPicker),
                action: Action::Cancel,
            },
            Binding {
                key: KeySpec::new(KeyCode::Char(' '), KeyModifiers::NONE),
                context: ContextMatcher::Overlay(OverlayKind::TagPicker),
                action: Action::Toggle,
            },
            Binding {
                key: KeySpec::new(KeyCode::Down, KeyModifiers::NONE),
                context: ContextMatcher::Overlay(OverlayKind::TagPicker),
                action: Action::ListDown,
            },
            Binding {
                key: KeySpec::new(KeyCode::Up, KeyModifiers::NONE),
                context: ContextMatcher::Overlay(OverlayKind::TagPicker),
                action: Action::ListUp,
            },
            Binding {
                key: KeySpec::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
                context: ContextMatcher::Overlay(OverlayKind::TagPicker),
                action: Action::ListDown,
            },
            Binding {
                key: KeySpec::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
                context: ContextMatcher::Overlay(OverlayKind::TagPicker),
                action: Action::ListUp,
            },
            // ---- Base(Search) ----
            Binding {
                key: KeySpec::new(KeyCode::Enter, KeyModifiers::NONE),
                context: ContextMatcher::Base(BaseModeKind::Search),
                action: Action::Submit,
            },
            Binding {
                key: KeySpec::new(KeyCode::Esc, KeyModifiers::NONE),
                context: ContextMatcher::Base(BaseModeKind::Search),
                action: Action::Cancel,
            },
            Binding {
                key: KeySpec::new(KeyCode::Down, KeyModifiers::NONE),
                context: ContextMatcher::Base(BaseModeKind::Search),
                action: Action::ListDown,
            },
            Binding {
                key: KeySpec::new(KeyCode::Up, KeyModifiers::NONE),
                context: ContextMatcher::Base(BaseModeKind::Search),
                action: Action::ListUp,
            },
            Binding {
                key: KeySpec::new(KeyCode::Char('n'), KeyModifiers::CONTROL),
                context: ContextMatcher::Base(BaseModeKind::Search),
                action: Action::ListDown,
            },
            Binding {
                key: KeySpec::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
                context: ContextMatcher::Base(BaseModeKind::Search),
                action: Action::ListUp,
            },
            // ---- Base(NoteView) ----
            Binding {
                key: KeySpec::new(KeyCode::Esc, KeyModifiers::NONE),
                context: ContextMatcher::Base(BaseModeKind::NoteView),
                action: Action::Cancel,
            },
            Binding {
                key: KeySpec::new(KeyCode::Enter, KeyModifiers::NONE),
                context: ContextMatcher::Base(BaseModeKind::NoteView),
                action: Action::Edit(EditOp::Newline),
            },
            Binding {
                key: KeySpec::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
                context: ContextMatcher::Base(BaseModeKind::NoteView),
                action: Action::ForceSave,
            },
            Binding {
                key: KeySpec::new(KeyCode::Char(';'), KeyModifiers::ALT),
                context: ContextMatcher::Base(BaseModeKind::NoteView),
                action: Action::OpenTagPicker,
            },
        ];
        // Insert Alt+digit bindings for NoteView (ToggleTagAt 1..9, 0->9).
        let mut bindings = bindings;
        for (idx, ch) in ('1'..='9').enumerate() {
            bindings.push(Binding {
                key: KeySpec::new(KeyCode::Char(ch), KeyModifiers::ALT),
                context: ContextMatcher::Base(BaseModeKind::NoteView),
                action: Action::ToggleTagAt(idx),
            });
        }
        bindings.push(Binding {
            key: KeySpec::new(KeyCode::Char('0'), KeyModifiers::ALT),
            context: ContextMatcher::Base(BaseModeKind::NoteView),
            action: Action::ToggleTagAt(9),
        });
        Self { bindings }
    }
}

pub fn translate_edit_line(event: &KeyEvent) -> Option<EditOp> {
    let m = event.modifiers;
    match event.code {
        KeyCode::Char(c) => {
            if m.is_empty() || m == KeyModifiers::SHIFT {
                Some(EditOp::Insert(c))
            } else if m == KeyModifiers::CONTROL {
                match c {
                    'a' => Some(EditOp::Home),
                    'e' => Some(EditOp::End),
                    'f' => Some(EditOp::Right),
                    'b' => Some(EditOp::Left),
                    'd' => Some(EditOp::Delete),
                    'w' => Some(EditOp::DeleteWordBack),
                    _ => None,
                }
            } else if m == KeyModifiers::ALT {
                match c {
                    'f' => Some(EditOp::WordRight),
                    'b' => Some(EditOp::WordLeft),
                    'd' => Some(EditOp::DeleteWordForward),
                    _ => None,
                }
            } else {
                None
            }
        }
        KeyCode::Backspace => {
            if m == KeyModifiers::ALT {
                Some(EditOp::DeleteWordBack)
            } else {
                Some(EditOp::Backspace)
            }
        }
        KeyCode::Delete => Some(EditOp::Delete),
        KeyCode::Left => Some(EditOp::Left),
        KeyCode::Right => Some(EditOp::Right),
        KeyCode::Home => Some(EditOp::Home),
        KeyCode::End => Some(EditOp::End),
        _ => None,
    }
}

pub fn translate_edit_multiline(event: &KeyEvent) -> Option<EditOp> {
    let m = event.modifiers;
    match event.code {
        KeyCode::Down => return Some(EditOp::Down),
        KeyCode::Up => return Some(EditOp::Up),
        KeyCode::Char('n') if m == KeyModifiers::CONTROL => {
            return Some(EditOp::Down);
        }
        KeyCode::Char('p') if m == KeyModifiers::CONTROL => {
            return Some(EditOp::Up);
        }
        _ => {}
    }
    translate_edit_line(event)
}
