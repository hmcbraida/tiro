//! Session persistence. One JSON file per session under
//! `~/.local/tiro/sessions/<id>.json`. Saves are message-by-message so a
//! crash mid-turn loses at most the in-flight partial.

use std::fs;
use std::io;
use std::path::PathBuf;

use crate::agent::session::AgentSession;

pub trait SessionStore: Send + Sync {
    fn list(&self) -> io::Result<Vec<AgentSession>>;
    fn load(&self, id: &str) -> io::Result<AgentSession>;
    fn save(&self, session: &AgentSession) -> io::Result<()>;
}

pub struct DirectorySessionStore {
    root: PathBuf,
}

impl DirectorySessionStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.json"))
    }
}

impl SessionStore for DirectorySessionStore {
    fn list(&self) -> io::Result<Vec<AgentSession>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let raw = match fs::read_to_string(&path) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let Ok(session) = serde_json::from_str::<AgentSession>(&raw) else {
                continue;
            };
            out.push(session);
        }
        // Newest first.
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(out)
    }

    fn load(&self, id: &str) -> io::Result<AgentSession> {
        let raw = fs::read_to_string(self.path_for(id))?;
        serde_json::from_str(&raw).map_err(io::Error::other)
    }

    fn save(&self, session: &AgentSession) -> io::Result<()> {
        fs::create_dir_all(&self.root)?;
        let body =
            serde_json::to_string_pretty(session).map_err(io::Error::other)?;
        fs::write(self.path_for(&session.id), body)
    }
}
