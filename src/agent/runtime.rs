//! Agent runtime: owns the LLM backend + session store, and drives one
//! streaming turn at a time when the UI asks for it.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use llm::{
    LLMProvider,
    builder::{LLMBackend, LLMBuilder},
    chat::{
        ChatMessage, ChatRole, FunctionTool, MessageType, StreamChunk, Tool,
    },
};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use crate::agent::config::{AgentConfig, Provider};
use crate::agent::session::{AgentSession, TranscriptMessage};
use crate::agent::store::{DirectorySessionStore, SessionStore};
use crate::agent::tools::{ToolSpec, all_specs, dispatch as dispatch_tool};
use crate::engine::TiroEngine;
use crate::store::NoteStore;

pub enum AgentRuntime {
    Disabled,
    Enabled(EnabledAgent),
}

/// `Box<dyn LLMProvider>` is technically not `Send + Sync` because the
/// trait object discards its supertrait bounds -- even though every concrete
/// impl is required to be `Send + Sync` (via `ChatProvider: Send + Sync`).
/// The wrapper makes the bound explicit so the value can cross task
/// boundaries via `Arc`.
pub(crate) struct SendableProvider(pub Box<dyn LLMProvider>);
unsafe impl Send for SendableProvider {}
unsafe impl Sync for SendableProvider {}

pub struct EnabledAgent {
    pub(crate) backend: Arc<SendableProvider>,
    #[allow(dead_code)]
    pub system_prompt: String,
    pub session_store: Arc<dyn SessionStore>,
    #[allow(dead_code)]
    pub model_label: String,
    #[allow(dead_code)]
    pub provider_label: &'static str,
}

impl AgentRuntime {
    /// Build a runtime from a resolved [`AgentConfig`]. `sessions_dir` is
    /// where session JSON files live.
    pub fn new(
        cfg: AgentConfig,
        sessions_dir: PathBuf,
    ) -> Result<Self, RuntimeBuildError> {
        if matches!(cfg.provider, Provider::None) {
            return Ok(AgentRuntime::Disabled);
        }
        let backend = build_backend(&cfg)?;
        let store = DirectorySessionStore::new(sessions_dir);
        Ok(AgentRuntime::Enabled(EnabledAgent {
            backend: Arc::new(SendableProvider(backend)),
            system_prompt: cfg.system_prompt,
            session_store: Arc::new(store),
            model_label: cfg.model,
            provider_label: cfg.provider.label(),
        }))
    }

    #[allow(dead_code)]
    pub fn is_enabled(&self) -> bool {
        matches!(self, AgentRuntime::Enabled(_))
    }

    pub fn enabled(&self) -> Option<&EnabledAgent> {
        match self {
            AgentRuntime::Enabled(e) => Some(e),
            AgentRuntime::Disabled => None,
        }
    }
}

fn build_backend(
    cfg: &AgentConfig,
) -> Result<Box<dyn LLMProvider>, RuntimeBuildError> {
    let backend = match cfg.provider {
        Provider::OpenAi => LLMBackend::OpenAI,
        Provider::Anthropic => LLMBackend::Anthropic,
        Provider::Ollama => LLMBackend::Ollama,
        Provider::None => unreachable!(),
    };
    let mut builder = LLMBuilder::new()
        .backend(backend)
        .model(&cfg.model)
        .system(&cfg.system_prompt);
    if let Some(key) = &cfg.api_key {
        builder = builder.api_key(key);
    }
    if matches!(cfg.provider, Provider::Ollama) {
        builder = builder.base_url(&cfg.ollama_url);
    }
    builder.build().map_err(RuntimeBuildError::Llm)
}

#[derive(Debug)]
pub enum RuntimeBuildError {
    Llm(llm::error::LLMError),
}

impl std::fmt::Display for RuntimeBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeBuildError::Llm(e) => write!(f, "llm backend: {e}"),
        }
    }
}

impl std::error::Error for RuntimeBuildError {}

/// Events the agent task emits while running a turn. The UI consumes
/// these on the main thread.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    Token(String),
    AssistantMessageComplete,
    ToolCall {
        name: String,
        args: Value,
    },
    ToolResult {
        name: String,
        result: Value,
        is_error: bool,
    },
    Error(String),
    Done,
}

/// Cancellation handle for an in-flight turn.
pub struct TurnHandle {
    pub cancel: oneshot::Sender<()>,
}

/// Spawn one streaming turn on the current tokio runtime. The session is
/// mutated in place via the store; the UI receives transcript updates via
/// `tx` and can cancel via the returned [`TurnHandle`].
pub fn spawn_turn<S: NoteStore + Send + 'static>(
    agent: &EnabledAgent,
    engine: Arc<Mutex<TiroEngine<S>>>,
    session: AgentSession,
    user_message: String,
    note_preamble: Option<String>,
    tx: mpsc::UnboundedSender<AgentEvent>,
) -> (TurnHandle, tokio::task::JoinHandle<AgentSession>) {
    let backend = agent.backend.clone();
    let store = agent.session_store.clone();
    let (cancel_tx, cancel_rx) = oneshot::channel();

    let join = tokio::spawn(async move {
        run_turn(
            backend,
            store,
            engine,
            session,
            user_message,
            note_preamble,
            tx,
            cancel_rx,
        )
        .await
    });

    (TurnHandle { cancel: cancel_tx }, join)
}

#[allow(clippy::too_many_arguments)]
async fn run_turn<S: NoteStore + Send + 'static>(
    backend: Arc<SendableProvider>,
    store: Arc<dyn SessionStore>,
    engine: Arc<Mutex<TiroEngine<S>>>,
    mut session: AgentSession,
    user_message: String,
    note_preamble: Option<String>,
    tx: mpsc::UnboundedSender<AgentEvent>,
    mut cancel_rx: oneshot::Receiver<()>,
) -> AgentSession {
    // Persist the user turn before kicking off the model so it's recoverable.
    let user_full = match note_preamble {
        Some(p) if !p.is_empty() => format!("{p}\n\n{user_message}"),
        _ => user_message,
    };
    session.messages.push(TranscriptMessage::User(user_full));
    let _ = store.save(&session);

    let tools = build_llm_tools();

    loop {
        let history = build_chat_history(&session);
        let provider: &dyn LLMProvider = backend.0.as_ref();
        let stream_result = provider
            .chat_stream_with_tools(&history, Some(&tools))
            .await;
        let mut stream = match stream_result {
            Ok(s) => s,
            Err(e) => {
                push_error(&mut session, &store, &tx, format!("provider: {e}"));
                break;
            }
        };

        let mut assistant_buf = String::new();
        let mut pending_tool_calls: Vec<(String, String, Value)> = Vec::new();
        // Indices in `pending_tool_calls` keyed by content-block index, used
        // to assemble JSON deltas before the call is complete.
        let mut partial_args: std::collections::HashMap<usize, String> =
            std::collections::HashMap::new();
        let mut block_index: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::new();

        let cancelled = loop {
            tokio::select! {
                biased;
                _ = &mut cancel_rx => break true,
                next = stream.next() => match next {
                    None => break false,
                    Some(Err(e)) => {
                        push_error(&mut session, &store, &tx, format!("stream: {e}"));
                        return session;
                    }
                    Some(Ok(chunk)) => {
                        consume_chunk(
                            chunk,
                            &mut assistant_buf,
                            &mut pending_tool_calls,
                            &mut partial_args,
                            &mut block_index,
                            &tx,
                        );
                    }
                }
            }
        };

        if cancelled {
            push_error(&mut session, &store, &tx, "turn cancelled".into());
            break;
        }

        // Finalise any tool calls that only delivered partial JSON.
        for (idx, slot) in &block_index {
            if let Some(raw) = partial_args.get(idx)
                && let Some((_id, _name, args)) =
                    pending_tool_calls.get_mut(*slot)
                && matches!(args, Value::Null)
            {
                *args = serde_json::from_str(raw).unwrap_or(Value::Null);
            }
        }

        if !assistant_buf.is_empty() {
            session
                .messages
                .push(TranscriptMessage::Assistant(assistant_buf.clone()));
            let _ = tx.send(AgentEvent::AssistantMessageComplete);
            let _ = store.save(&session);
        }

        if pending_tool_calls.is_empty() {
            break;
        }

        for (_call_id, name, args) in pending_tool_calls {
            let _ = tx.send(AgentEvent::ToolCall {
                name: name.clone(),
                args: args.clone(),
            });
            session.messages.push(TranscriptMessage::ToolCall {
                name: name.clone(),
                args: args.clone(),
            });
            let (result, is_error) = dispatch_tool(&engine, &name, &args);
            let _ = tx.send(AgentEvent::ToolResult {
                name: name.clone(),
                result: result.clone(),
                is_error,
            });
            session.messages.push(TranscriptMessage::ToolResult {
                name,
                result,
                is_error,
            });
            let _ = store.save(&session);
        }
        // Loop: feed tool results back to the model.
    }

    let _ = tx.send(AgentEvent::Done);
    session
}

fn push_error(
    session: &mut AgentSession,
    store: &Arc<dyn SessionStore>,
    tx: &mpsc::UnboundedSender<AgentEvent>,
    msg: String,
) {
    let _ = tx.send(AgentEvent::Error(msg.clone()));
    session.messages.push(TranscriptMessage::SystemError(msg));
    let _ = store.save(session);
}

fn build_chat_history(session: &AgentSession) -> Vec<ChatMessage> {
    let mut out: Vec<ChatMessage> = Vec::new();
    for m in &session.messages {
        match m {
            TranscriptMessage::User(text) => {
                out.push(ChatMessage {
                    role: ChatRole::User,
                    message_type: MessageType::Text,
                    content: text.clone(),
                });
            }
            TranscriptMessage::Assistant(text) => {
                out.push(ChatMessage {
                    role: ChatRole::Assistant,
                    message_type: MessageType::Text,
                    content: text.clone(),
                });
            }
            TranscriptMessage::ToolResult { name, result, .. } => {
                // No native Tool role -- inject as a user-context message.
                out.push(ChatMessage {
                    role: ChatRole::User,
                    message_type: MessageType::Text,
                    content: format!(
                        "[tool result: {name}]\n{}",
                        serde_json::to_string(result).unwrap_or_default()
                    ),
                });
            }
            TranscriptMessage::ToolCall { .. }
            | TranscriptMessage::SystemError(_) => {}
        }
    }
    out
}

fn build_llm_tools() -> Vec<Tool> {
    all_specs().into_iter().map(spec_to_tool).collect()
}

fn spec_to_tool(spec: ToolSpec) -> Tool {
    Tool {
        tool_type: "function".to_string(),
        function: FunctionTool {
            name: spec.name.to_string(),
            description: spec.description.to_string(),
            parameters: spec.parameters,
        },
        cache_control: None,
    }
}

fn consume_chunk(
    chunk: StreamChunk,
    text_buf: &mut String,
    tool_calls: &mut Vec<(String, String, Value)>,
    partial_args: &mut std::collections::HashMap<usize, String>,
    block_index: &mut std::collections::HashMap<usize, usize>,
    tx: &mpsc::UnboundedSender<AgentEvent>,
) {
    match chunk {
        StreamChunk::Text(text) => {
            text_buf.push_str(&text);
            let _ = tx.send(AgentEvent::Token(text));
        }
        StreamChunk::ToolUseStart { index, id, name } => {
            let slot = tool_calls.len();
            tool_calls.push((id, name, Value::Null));
            block_index.insert(index, slot);
            partial_args.entry(index).or_default();
        }
        StreamChunk::ToolUseInputDelta {
            index,
            partial_json,
        } => {
            partial_args
                .entry(index)
                .or_default()
                .push_str(&partial_json);
        }
        StreamChunk::ToolUseComplete { index, tool_call } => {
            let args: Value =
                serde_json::from_str(&tool_call.function.arguments)
                    .unwrap_or(Value::Null);
            if let Some(slot) = block_index.get(&index) {
                if let Some(entry) = tool_calls.get_mut(*slot) {
                    entry.0 = tool_call.id.clone();
                    entry.1 = tool_call.function.name.clone();
                    entry.2 = args;
                }
            } else {
                tool_calls.push((tool_call.id, tool_call.function.name, args));
            }
        }
        StreamChunk::Done { .. } => {}
    }
}
