//! Agent runtime: owns one rig provider client per configured backend
//! plus the session store, and drives one streaming turn at a time when
//! the UI asks for it.
//!
//! Rig's `Agent::stream_chat` runs the multi-turn tool loop internally;
//! we just translate its events back onto our existing transcript +
//! [`AgentEvent`] shape so the rest of the app is untouched.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use rig::agent::MultiTurnStreamItem;
use rig::client::CompletionClient;
use rig::completion::{CompletionModel, GetTokenUsage, Message};
use rig::message::{Text, ToolResultContent, UserContent};
use rig::providers::{anthropic, ollama, openai};
use rig::streaming::{
    StreamedAssistantContent, StreamedUserContent, StreamingChat,
};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use crate::agent::config::{AgentConfig, Provider};
use crate::agent::session::{AgentSession, TranscriptMessage};
use crate::agent::store::{DirectorySessionStore, SessionStore};
use crate::agent::tools::build_tools;
use crate::engine::TiroEngine;
use crate::store::NoteStore;

const MAX_TURNS: usize = 8;

pub enum AgentRuntime {
    Disabled,
    Enabled(EnabledAgent),
}

/// One configured provider client. Each variant is a rig client that
/// implements [`CompletionClient`]; the variant is chosen once at config
/// load and dispatched on per turn.
pub enum BackendClient {
    OpenAi(openai::Client),
    Anthropic(anthropic::Client),
    Ollama(ollama::Client),
}

pub struct EnabledAgent {
    pub(crate) backend: Arc<BackendClient>,
    pub system_prompt: String,
    pub session_store: Arc<dyn SessionStore>,
    pub model: String,
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
            backend: Arc::new(backend),
            system_prompt: cfg.system_prompt,
            session_store: Arc::new(store),
            model: cfg.model,
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
) -> Result<BackendClient, RuntimeBuildError> {
    match cfg.provider {
        Provider::OpenAi => {
            let key = cfg.api_key.as_deref().unwrap_or("");
            openai::Client::new(key)
                .map(BackendClient::OpenAi)
                .map_err(|e| RuntimeBuildError::Client(e.to_string()))
        }
        Provider::Anthropic => {
            let key = cfg.api_key.as_deref().unwrap_or("");
            anthropic::Client::new(key)
                .map(BackendClient::Anthropic)
                .map_err(|e| RuntimeBuildError::Client(e.to_string()))
        }
        Provider::Ollama => ollama::Client::builder()
            .api_key(rig::client::Nothing)
            .base_url(&cfg.ollama_url)
            .build()
            .map(BackendClient::Ollama)
            .map_err(|e| RuntimeBuildError::Client(e.to_string())),
        Provider::None => unreachable!(),
    }
}

#[derive(Debug)]
pub enum RuntimeBuildError {
    Client(String),
}

impl std::fmt::Display for RuntimeBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeBuildError::Client(e) => write!(f, "rig client: {e}"),
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

/// Spawn one streaming turn on the current tokio runtime. The caller must
/// have already appended the user message (with any preamble) to
/// `session`; this function persists that state, drives the model, and
/// streams transcript updates via `tx`. Cancel via the returned
/// [`TurnHandle`].
pub fn spawn_turn<S: NoteStore + Send + 'static>(
    agent: &EnabledAgent,
    engine: Arc<Mutex<TiroEngine<S>>>,
    session: AgentSession,
    user_message: String,
    tx: mpsc::UnboundedSender<AgentEvent>,
) -> (TurnHandle, tokio::task::JoinHandle<AgentSession>) {
    let backend = agent.backend.clone();
    let store = agent.session_store.clone();
    let system_prompt = agent.system_prompt.clone();
    let model = agent.model.clone();
    let (cancel_tx, cancel_rx) = oneshot::channel();

    let join = tokio::spawn(async move {
        run_turn(
            backend,
            store,
            engine,
            system_prompt,
            model,
            session,
            user_message,
            tx,
            cancel_rx,
        )
        .await
    });

    (TurnHandle { cancel: cancel_tx }, join)
}

#[allow(clippy::too_many_arguments)]
async fn run_turn<S: NoteStore + Send + 'static>(
    backend: Arc<BackendClient>,
    store: Arc<dyn SessionStore>,
    engine: Arc<Mutex<TiroEngine<S>>>,
    system_prompt: String,
    model: String,
    mut session: AgentSession,
    user_message: String,
    tx: mpsc::UnboundedSender<AgentEvent>,
    cancel_rx: oneshot::Receiver<()>,
) -> AgentSession {
    let _ = store.save(&session);

    let history = build_chat_history(&session);
    let prompt_msg = Message::user(&user_message);

    match backend.as_ref() {
        BackendClient::OpenAi(c) => {
            drive_turn(
                c,
                &model,
                &system_prompt,
                engine,
                prompt_msg,
                history,
                &mut session,
                &store,
                &tx,
                cancel_rx,
            )
            .await;
        }
        BackendClient::Anthropic(c) => {
            drive_turn(
                c,
                &model,
                &system_prompt,
                engine,
                prompt_msg,
                history,
                &mut session,
                &store,
                &tx,
                cancel_rx,
            )
            .await;
        }
        BackendClient::Ollama(c) => {
            drive_turn(
                c,
                &model,
                &system_prompt,
                engine,
                prompt_msg,
                history,
                &mut session,
                &store,
                &tx,
                cancel_rx,
            )
            .await;
        }
    }

    let _ = tx.send(AgentEvent::Done);
    session
}

#[allow(clippy::too_many_arguments)]
async fn drive_turn<C, S>(
    client: &C,
    model: &str,
    system_prompt: &str,
    engine: Arc<Mutex<TiroEngine<S>>>,
    prompt: Message,
    history: Vec<Message>,
    session: &mut AgentSession,
    store: &Arc<dyn SessionStore>,
    tx: &mpsc::UnboundedSender<AgentEvent>,
    mut cancel_rx: oneshot::Receiver<()>,
) where
    C: CompletionClient,
    C::CompletionModel: CompletionModel + 'static,
    <C::CompletionModel as CompletionModel>::StreamingResponse: GetTokenUsage,
    S: NoteStore + Send + 'static,
{
    let agent = client
        .agent(model)
        .preamble(system_prompt)
        .default_max_turns(MAX_TURNS)
        .tools(build_tools(engine))
        .build();

    let mut stream = agent.stream_chat(prompt, history).await;

    // Buffer streamed assistant text so we can flush it as a single
    // Assistant transcript entry when the model transitions to a tool
    // call or finishes the turn.
    let mut text_buf = String::new();
    // Map rig's internal_call_id → tool name, so when a ToolResult event
    // arrives (which only carries an id) we can name the corresponding
    // transcript entry.
    let mut call_names: HashMap<String, String> = HashMap::new();

    loop {
        tokio::select! {
            biased;
            _ = &mut cancel_rx => {
                push_error(session, store, tx, "turn cancelled".into());
                return;
            }
            next = stream.next() => match next {
                None => break,
                Some(Err(e)) => {
                    push_error(session, store, tx, format!("stream: {e}"));
                    return;
                }
                Some(Ok(item)) => {
                    handle_item(
                        item,
                        &mut text_buf,
                        &mut call_names,
                        session,
                        store,
                        tx,
                    );
                }
            }
        }
    }

    flush_text(&mut text_buf, session, store, tx);
}

fn handle_item(
    item: MultiTurnStreamItem<impl Clone>,
    text_buf: &mut String,
    call_names: &mut HashMap<String, String>,
    session: &mut AgentSession,
    store: &Arc<dyn SessionStore>,
    tx: &mpsc::UnboundedSender<AgentEvent>,
) {
    match item {
        MultiTurnStreamItem::StreamAssistantItem(content) => match content {
            StreamedAssistantContent::Text(Text { text, .. }) => {
                text_buf.push_str(&text);
                let _ = tx.send(AgentEvent::Token(text));
            }
            StreamedAssistantContent::ToolCall {
                tool_call,
                internal_call_id,
            } => {
                flush_text(text_buf, session, store, tx);
                let name = tool_call.function.name.clone();
                let args = tool_call.function.arguments.clone();
                call_names.insert(internal_call_id, name.clone());
                let _ = tx.send(AgentEvent::ToolCall {
                    name: name.clone(),
                    args: args.clone(),
                });
                session
                    .messages
                    .push(TranscriptMessage::ToolCall { name, args });
                let _ = store.save(session);
            }
            // Tool-call deltas, reasoning, and per-turn final responses
            // don't surface to the UI: we already emit text via the Text
            // arm and the multi-turn FinalResponse below.
            _ => {}
        },
        MultiTurnStreamItem::StreamUserItem(
            StreamedUserContent::ToolResult {
                tool_result,
                internal_call_id,
            },
        ) => {
            let name = call_names
                .remove(&internal_call_id)
                .unwrap_or_else(|| "unknown".to_string());
            let raw = tool_result_to_text(&tool_result.content);
            let (result, is_error) = decode_tool_result(&raw);
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
            let _ = store.save(session);
        }
        MultiTurnStreamItem::CompletionCall(_) => {}
        MultiTurnStreamItem::FinalResponse(_) => {
            flush_text(text_buf, session, store, tx);
        }
        _ => {}
    }
}

fn flush_text(
    text_buf: &mut String,
    session: &mut AgentSession,
    store: &Arc<dyn SessionStore>,
    tx: &mpsc::UnboundedSender<AgentEvent>,
) {
    if text_buf.is_empty() {
        return;
    }
    let body = std::mem::take(text_buf);
    session.messages.push(TranscriptMessage::Assistant(body));
    let _ = tx.send(AgentEvent::AssistantMessageComplete);
    let _ = store.save(session);
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

/// Reduce a [`OneOrMany<ToolResultContent>`] to a single string. Images
/// are skipped because our tools never produce them; if one ever does,
/// we record a placeholder rather than panicking.
fn tool_result_to_text(content: &rig::OneOrMany<ToolResultContent>) -> String {
    let mut out = String::new();
    for part in content.iter() {
        match part {
            ToolResultContent::Text(t) => out.push_str(&t.text),
            ToolResultContent::Image(_) => out.push_str("[image]"),
        }
    }
    out
}

/// Parse the model-facing tool output back into JSON. Our tools always
/// serialise to JSON, but rig also accepts raw text from foreign tools,
/// so fall back to a wrapped string when parsing fails.
fn decode_tool_result(raw: &str) -> (Value, bool) {
    let value: Value = serde_json::from_str(raw)
        .unwrap_or_else(|_| Value::String(raw.to_string()));
    let is_error = value.as_object().is_some_and(|o| o.contains_key("error"));
    (value, is_error)
}

/// Translate the persistent transcript into rig [`Message`]s for replay.
/// Tool calls are dropped (rig manages tool history within a turn) and
/// tool results are folded into user-context messages, matching the
/// previous behaviour.
fn build_chat_history(session: &AgentSession) -> Vec<Message> {
    // The last User message in the transcript is the prompt we just
    // pushed; skip it so it isn't duplicated when stream_chat is called
    // with `prompt` + this history.
    let mut history: Vec<Message> = Vec::new();
    let messages = &session.messages;
    let prompt_idx = messages
        .iter()
        .rposition(|m| matches!(m, TranscriptMessage::User(_)));
    for (i, m) in messages.iter().enumerate() {
        if Some(i) == prompt_idx {
            continue;
        }
        match m {
            TranscriptMessage::User(text) => {
                history.push(Message::user(text));
            }
            TranscriptMessage::Assistant(text) => {
                history.push(Message::assistant(text));
            }
            TranscriptMessage::ToolResult { name, result, .. } => {
                let body = format!(
                    "[tool result: {name}]\n{}",
                    serde_json::to_string(result).unwrap_or_default()
                );
                history.push(Message::User {
                    content: rig::OneOrMany::one(UserContent::text(body)),
                });
            }
            TranscriptMessage::ToolCall { .. }
            | TranscriptMessage::SystemError(_) => {}
        }
    }
    history
}
