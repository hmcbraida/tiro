mod agent;
mod engine;
mod error;
mod filter;
mod note;
mod setup;
mod store;
mod ui;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use agent::AgentRuntime;
use engine::TiroEngine;
use store::DirectoryNoteStore;

#[tokio::main(flavor = "multi_thread")]
async fn main() -> std::io::Result<()> {
    // note: this currently panics if HOME isn't set.
    // RIP windows users.
    let root = tiro_root();
    let notes_dir = root.join("notes");
    let sessions_dir = root.join("sessions");
    std::fs::create_dir_all(&notes_dir)?;
    std::fs::create_dir_all(&sessions_dir)?;

    let config_path = root.join("config.toml");
    let loaded = match setup::load_config(&config_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!(
                "failed to load config at {}: {e}",
                config_path.display()
            );
            std::process::exit(1);
        }
    };

    let runtime = match AgentRuntime::new(loaded.agent, sessions_dir) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("failed to build agent runtime: {e}");
            std::process::exit(1);
        }
    };

    let store = DirectoryNoteStore::new(notes_dir);
    let engine = TiroEngine::new(store, loaded.tags);
    let engine = Arc::new(Mutex::new(engine));

    ui::run(engine, runtime).await
}

fn tiro_root() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".local/tiro")
}
