mod engine;
mod error;
mod note;
mod store;
mod ui;

use std::path::PathBuf;

use engine::TiroEngine;
use note::Tag;
use store::DirectoryNoteStore;

fn main() -> std::io::Result<()> {
    let root = note_dir();
    std::fs::create_dir_all(&root)?;

    let tags = seed_tags();
    let store = DirectoryNoteStore::new(root);
    let engine = TiroEngine::new(store, tags);
    ui::run(engine)
}

fn note_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".local/tiro/notes")
}

fn seed_tags() -> Vec<Tag> {
    [
        "todo",
        "idea",
        "work",
        "personal",
        "journal",
        "research",
        "follow-up",
        "reading",
        "shopping",
        "later",
    ]
    .into_iter()
    .map(|s| Tag::new(s.to_string()))
    .collect()
}
