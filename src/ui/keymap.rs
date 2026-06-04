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
            // ---- Ctrl+/ -> Open agent modal ----
            // Requires a terminal with the kitty keyboard protocol; on
            // legacy terminals Ctrl+/ collapses to Ctrl+_ at byte 0x1F.
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

// ---------------------------------------------------------------------
// Config-string parser for [`KeySpec`].
//
// Syntax mirrors Helix:
//
//   "C-w"          Ctrl+w
//   "A-ret"        Alt+Enter
//   "C-A-S-F12"    Ctrl+Alt+Shift+F12
//   "space"        Space
//   "/"            literal slash
//   "C-minus"      Ctrl+-
//
// Modifiers (prefix, hyphen-separated, any order): `S` Shift, `A` Alt,
// `C` Ctrl, `Meta`/`Cmd`/`Win` Super.
//
// Named keys: ret, tab, esc, space, backspace, del, ins, up, down,
// left, right, home, end, pageup, pagedown, minus, lt, gt, f1..f24.
//
// Normalization: shift + ASCII lowercase letter collapses to uppercase
// without SHIFT (so `C-S-a` == `C-A`), matching how legacy terminals
// encode the key. Named-key tokens are case-insensitive.
// ---------------------------------------------------------------------

#[allow(dead_code)] // wired into config loading in a follow-up
#[derive(Debug, PartialEq, Eq)]
pub enum ParseKeyError {
    Empty,
    UnknownModifier(String),
    UnknownKey(String),
    MissingKey,
}

impl std::fmt::Display for ParseKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseKeyError::Empty => write!(f, "empty key binding"),
            ParseKeyError::UnknownModifier(s) => {
                write!(f, "unknown modifier '{s}'")
            }
            ParseKeyError::UnknownKey(s) => write!(f, "unknown key '{s}'"),
            ParseKeyError::MissingKey => {
                write!(f, "binding has modifiers but no key")
            }
        }
    }
}

impl std::error::Error for ParseKeyError {}

#[allow(dead_code)] // wired into config loading in a follow-up
pub fn parse_key(s: &str) -> Result<KeySpec, ParseKeyError> {
    if s.is_empty() {
        return Err(ParseKeyError::Empty);
    }
    // A single character -- including '-' itself -- is always a literal key.
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if chars.next().is_none() {
        return Ok(normalize(KeySpec::new(
            KeyCode::Char(first),
            KeyModifiers::NONE,
        )));
    }
    // Otherwise split on '-'. Every token before the last is a modifier;
    // the last is the key. To bind Ctrl+minus, write `C-minus`.
    let parts: Vec<&str> = s.split('-').collect();
    let (key_token, mod_tokens) = parts
        .split_last()
        .map(|(last, rest)| (*last, rest))
        .ok_or(ParseKeyError::Empty)?;
    if key_token.is_empty() {
        return Err(ParseKeyError::MissingKey);
    }
    let mut modifiers = KeyModifiers::NONE;
    for tok in mod_tokens {
        match *tok {
            "S" => modifiers |= KeyModifiers::SHIFT,
            "A" => modifiers |= KeyModifiers::ALT,
            "C" => modifiers |= KeyModifiers::CONTROL,
            "Meta" | "Cmd" | "Win" => modifiers |= KeyModifiers::SUPER,
            other => {
                return Err(ParseKeyError::UnknownModifier(other.to_string()));
            }
        }
    }
    let code = parse_keycode(key_token)?;
    Ok(normalize(KeySpec::new(code, modifiers)))
}

fn parse_keycode(token: &str) -> Result<KeyCode, ParseKeyError> {
    let mut chars = token.chars();
    let first = chars.next().ok_or(ParseKeyError::MissingKey)?;
    if chars.next().is_none() {
        return Ok(KeyCode::Char(first));
    }
    let lower = token.to_ascii_lowercase();
    let code = match lower.as_str() {
        "ret" | "enter" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "esc" | "escape" => KeyCode::Esc,
        "space" => KeyCode::Char(' '),
        "backspace" | "bs" => KeyCode::Backspace,
        "del" | "delete" => KeyCode::Delete,
        "ins" | "insert" => KeyCode::Insert,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "minus" => KeyCode::Char('-'),
        "lt" => KeyCode::Char('<'),
        "gt" => KeyCode::Char('>'),
        other if other.starts_with('f') => {
            let n: u8 = other[1..]
                .parse()
                .map_err(|_| ParseKeyError::UnknownKey(token.to_string()))?;
            if !(1..=24).contains(&n) {
                return Err(ParseKeyError::UnknownKey(token.to_string()));
            }
            KeyCode::F(n)
        }
        _ => return Err(ParseKeyError::UnknownKey(token.to_string())),
    };
    Ok(code)
}

fn normalize(spec: KeySpec) -> KeySpec {
    // Helix-style: Shift + ASCII lowercase folds to uppercase with no
    // SHIFT modifier -- that's what legacy terminals deliver anyway.
    if let KeyCode::Char(c) = spec.code
        && c.is_ascii_lowercase()
        && spec.mods.contains(KeyModifiers::SHIFT)
    {
        return KeySpec::new(
            KeyCode::Char(c.to_ascii_uppercase()),
            spec.mods - KeyModifiers::SHIFT,
        );
    }
    spec
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    fn k(code: KeyCode, mods: KeyModifiers) -> KeySpec {
        KeySpec::new(code, mods)
    }

    #[test]
    fn bare_chars() {
        assert_eq!(
            parse_key("a"),
            Ok(k(KeyCode::Char('a'), KeyModifiers::NONE))
        );
        assert_eq!(
            parse_key("/"),
            Ok(k(KeyCode::Char('/'), KeyModifiers::NONE))
        );
        assert_eq!(
            parse_key("-"),
            Ok(k(KeyCode::Char('-'), KeyModifiers::NONE))
        );
    }

    #[test]
    fn single_modifier() {
        assert_eq!(
            parse_key("C-w"),
            Ok(k(KeyCode::Char('w'), KeyModifiers::CONTROL))
        );
        assert_eq!(
            parse_key("A-x"),
            Ok(k(KeyCode::Char('x'), KeyModifiers::ALT))
        );
    }

    #[test]
    fn shift_lowercase_normalizes_to_uppercase() {
        assert_eq!(
            parse_key("S-a"),
            Ok(k(KeyCode::Char('A'), KeyModifiers::NONE))
        );
        assert_eq!(
            parse_key("C-S-a"),
            Ok(k(KeyCode::Char('A'), KeyModifiers::CONTROL))
        );
    }

    #[test]
    fn multi_modifier_any_order() {
        let want = k(
            KeyCode::F(12),
            KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT,
        );
        assert_eq!(parse_key("C-A-S-F12"), Ok(want.clone()));
        assert_eq!(parse_key("S-A-C-F12"), Ok(want));
    }

    #[test]
    fn named_keys() {
        assert_eq!(parse_key("ret"), Ok(k(KeyCode::Enter, KeyModifiers::NONE)));
        assert_eq!(
            parse_key("A-ret"),
            Ok(k(KeyCode::Enter, KeyModifiers::ALT))
        );
        assert_eq!(
            parse_key("space"),
            Ok(k(KeyCode::Char(' '), KeyModifiers::NONE))
        );
        assert_eq!(
            parse_key("C-minus"),
            Ok(k(KeyCode::Char('-'), KeyModifiers::CONTROL))
        );
    }

    #[test]
    fn ctrl_slash_is_distinct_from_ctrl_underscore() {
        // The parser treats these as separate identifiers; runtime
        // distinguishability depends on the kitty keyboard protocol.
        assert_eq!(
            parse_key("C-/"),
            Ok(k(KeyCode::Char('/'), KeyModifiers::CONTROL))
        );
        assert_eq!(
            parse_key("C-_"),
            Ok(k(KeyCode::Char('_'), KeyModifiers::CONTROL))
        );
        assert_ne!(parse_key("C-/"), parse_key("C-_"));
    }

    #[test]
    fn meta_aliases() {
        let want = k(KeyCode::Char('x'), KeyModifiers::SUPER);
        assert_eq!(parse_key("Meta-x"), Ok(want.clone()));
        assert_eq!(parse_key("Cmd-x"), Ok(want.clone()));
        assert_eq!(parse_key("Win-x"), Ok(want));
    }

    #[test]
    fn function_keys() {
        assert_eq!(parse_key("F1"), Ok(k(KeyCode::F(1), KeyModifiers::NONE)));
        assert_eq!(parse_key("f24"), Ok(k(KeyCode::F(24), KeyModifiers::NONE)));
        assert!(matches!(parse_key("F0"), Err(ParseKeyError::UnknownKey(_))));
        assert!(matches!(
            parse_key("F25"),
            Err(ParseKeyError::UnknownKey(_))
        ));
    }

    #[test]
    fn errors() {
        assert_eq!(parse_key(""), Err(ParseKeyError::Empty));
        assert!(matches!(
            parse_key("X-a"),
            Err(ParseKeyError::UnknownModifier(_))
        ));
        assert!(matches!(
            parse_key("C-bogus"),
            Err(ParseKeyError::UnknownKey(_))
        ));
        assert_eq!(parse_key("C-"), Err(ParseKeyError::MissingKey));
    }
}
