#[derive(Debug, Clone)]
pub enum Action {
    Edit(EditOp),

    // list navigation (search results, picker rows)
    ListUp,
    ListDown,

    // Enter / Esc dispatched per context
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

    // agent
    OpenAgentModal,
    SubmitAgentPrompt,
    AgentCancel,
    NewAgentSession,
    OpenSessionPicker,
    SubmitSessionPicker,
    ScrollTranscriptUp,
    ScrollTranscriptDown,
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
