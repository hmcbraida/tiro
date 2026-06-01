#[derive(Debug, Clone)]
pub enum Action {
    Edit(EditOp),

    // search / tag-picker list nav
    ListUp,
    ListDown,

    // Enter / Esc dispatched per mode
    Submit,
    Cancel,

    // global
    Quit,
    ForceSave,
    NewNote,
    OpenInEditor,
    StartCtrlX,
    CancelCtrlX,

    // note view
    ToggleTagAt(usize),
    OpenTagPicker,

    // tag picker
    Toggle,
}

#[derive(Debug, Clone)]
pub enum EditOp {
    Insert(char),
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    WordLeft,
    WordRight,
    Home,
    End,
    DeleteWordForward,
    DeleteWordBack,
    Newline,
}
