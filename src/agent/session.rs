//! Session model: one conversation between user and agent. Persisted by
//! [`crate::agent::store::SessionStore`].

use std::time::SystemTime;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: String,
    #[serde(with = "ts_rfc3339")]
    pub created_at: SystemTime,
    #[serde(default)]
    pub attached_note_ids: Vec<String>,
    #[serde(default)]
    pub messages: Vec<TranscriptMessage>,
}

impl AgentSession {
    pub fn new(id: String) -> Self {
        Self {
            id,
            created_at: SystemTime::now(),
            attached_note_ids: Vec::new(),
            messages: Vec::new(),
        }
    }

    /// One-line label for the session picker. Falls back to "(empty session)"
    /// when no turn has been taken yet.
    pub fn title(&self) -> String {
        for m in &self.messages {
            if let TranscriptMessage::User(text) = m {
                let first_line =
                    text.lines().next().unwrap_or("").trim().to_string();
                if !first_line.is_empty() {
                    return truncate(&first_line, 60);
                }
            }
        }
        "(empty session)".to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TranscriptMessage {
    User(String),
    Assistant(String),
    ToolCall {
        name: String,
        args: serde_json::Value,
    },
    ToolResult {
        name: String,
        result: serde_json::Value,
        is_error: bool,
    },
    SystemError(String),
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{head}…")
    }
}

mod ts_rfc3339 {
    use std::time::SystemTime;

    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(
        t: &SystemTime,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let dt: DateTime<Utc> = (*t).into();
        dt.to_rfc3339().serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<SystemTime, D::Error> {
        let s = String::deserialize(d)?;
        let dt = DateTime::parse_from_rfc3339(&s)
            .map_err(serde::de::Error::custom)?;
        Ok(dt.with_timezone(&Utc).into())
    }
}
