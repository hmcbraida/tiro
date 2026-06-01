use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::action::{Action, EditOp};
use super::state::{AppState, Mode};

pub fn translate(event: KeyEvent, state: &AppState) -> Option<Action> {
    let m = event.modifiers;

    if event.code == KeyCode::Char('c') && m.contains(KeyModifiers::CONTROL) {
        return Some(Action::Quit);
    }

    if state.ctrl_x_pending {
        return match event.code {
            KeyCode::Char('n') => Some(Action::NewNote),
            KeyCode::Char('e') => Some(Action::OpenInEditor),
            _ => Some(Action::CancelCtrlX),
        };
    }

    if event.code == KeyCode::Char('x') && m.contains(KeyModifiers::CONTROL) {
        return Some(Action::StartCtrlX);
    }

    match &state.mode {
        Mode::Search(_) => translate_search(event),
        Mode::NoteView(_) => translate_note_view(event),
        Mode::TagPicker(_) => translate_tag_picker(event),
    }
}

fn translate_search(event: KeyEvent) -> Option<Action> {
    let m = event.modifiers;
    match event.code {
        KeyCode::Enter => return Some(Action::Submit),
        KeyCode::Esc => return Some(Action::Cancel),
        KeyCode::Down => return Some(Action::ListDown),
        KeyCode::Up => return Some(Action::ListUp),
        KeyCode::Char('n') if m == KeyModifiers::CONTROL => {
            return Some(Action::ListDown);
        }
        KeyCode::Char('p') if m == KeyModifiers::CONTROL => {
            return Some(Action::ListUp);
        }
        _ => {}
    }
    translate_edit_line(event).map(Action::Edit)
}

fn translate_note_view(event: KeyEvent) -> Option<Action> {
    let m = event.modifiers;
    match event.code {
        KeyCode::Esc => return Some(Action::Cancel),
        KeyCode::Enter => return Some(Action::Edit(EditOp::Newline)),
        KeyCode::Char('s') if m == KeyModifiers::CONTROL => {
            return Some(Action::ForceSave);
        }
        KeyCode::Char(';') if m == KeyModifiers::ALT => {
            return Some(Action::OpenTagPicker);
        }
        KeyCode::Char(c) if m == KeyModifiers::ALT && c.is_ascii_digit() => {
            let idx = if c == '0' {
                9
            } else {
                (c as u8 - b'1') as usize
            };
            return Some(Action::ToggleTagAt(idx));
        }
        _ => {}
    }
    translate_edit_multiline(event).map(Action::Edit)
}

fn translate_tag_picker(event: KeyEvent) -> Option<Action> {
    let m = event.modifiers;
    match event.code {
        KeyCode::Enter => return Some(Action::Submit),
        KeyCode::Esc => return Some(Action::Cancel),
        KeyCode::Char(' ') if m.is_empty() || m == KeyModifiers::SHIFT => {
            return Some(Action::Toggle);
        }
        KeyCode::Down => return Some(Action::ListDown),
        KeyCode::Up => return Some(Action::ListUp),
        KeyCode::Char('n') if m == KeyModifiers::CONTROL => {
            return Some(Action::ListDown);
        }
        KeyCode::Char('p') if m == KeyModifiers::CONTROL => {
            return Some(Action::ListUp);
        }
        _ => {}
    }
    translate_edit_line(event).map(Action::Edit)
}

fn translate_edit_line(event: KeyEvent) -> Option<EditOp> {
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

fn translate_edit_multiline(event: KeyEvent) -> Option<EditOp> {
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
