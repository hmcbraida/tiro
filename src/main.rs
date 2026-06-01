mod engine;
mod error;
mod note;
mod setup;
mod store;
mod ui;

use std::path::PathBuf;

use engine::TiroEngine;
use store::DirectoryNoteStore;

fn main() -> std::io::Result<()> {
    // note: this currently panics if HOME isn't set.
    // RIP windows users.
    let root = tiro_root();
    // create the tiro state directory (and the notes subdir) if missing.
    let notes_dir = root.join("notes");
    std::fs::create_dir_all(&notes_dir)?;

    // Load the user's tag set from the TOML config file. The setup
    // module hides the on-disk shape of that file from us -- we just
    // hand it a path and get back ready-to-use Tag values.
    let config_path = root.join("config.toml");
    let tags = match setup::load_tags(&config_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!(
                "failed to load config at {}: {e}",
                config_path.display()
            );
            std::process::exit(1);
        }
    };

    // Initialise a directory store for the notes
    // centered on the notes subdirectory.
    let store = DirectoryNoteStore::new(notes_dir);
    // Set up the engine representing the application state.
    // The engine represents actions this application can take
    // to influence the outside world, and is what ties the UI
    // to any hooks or persistence.
    let engine = TiroEngine::new(store, tags);
    // Pass control over to the UI mainloop.
    ui::run(engine)
}

// The tiro root directory holds the user's config alongside the notes
// subdirectory
fn tiro_root() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".local/tiro")
}
