use std::{collections::HashSet, fs, path::PathBuf};

use crate::{
    error::{Result, TiroError},
    note::{Note, StoredNote},
};

pub trait NoteStore {
    fn list_all_notes(&self) -> Result<Vec<StoredNote>>;
    fn put_note(&mut self, note: StoredNote) -> Result<()>;
}

pub struct DirectoryNoteStore {
    root_directory: PathBuf,
}

impl DirectoryNoteStore {
    pub fn new(root_directory: PathBuf) -> Self {
        Self { root_directory }
    }
}

impl NoteStore for DirectoryNoteStore {
    fn list_all_notes(&self) -> Result<Vec<StoredNote>> {
        let mut notes = Vec::new();
        for entry in fs::read_dir(&self.root_directory)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("txt") {
                continue;
            }
            let id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| {
                    TiroError::Parse(format!(
                        "invalid filename: {}",
                        path.display()
                    ))
                })?
                .to_string();
            let raw = fs::read_to_string(&path)?;
            let Ok(note) = parse_note(&raw) else { continue };
            notes.push(StoredNote::new(id, note));
        }
        Ok(notes)
    }

    fn put_note(&mut self, note: StoredNote) -> Result<()> {
        fs::create_dir_all(&self.root_directory)?;
        let path = self.root_directory.join(format!("{}.txt", note.id));
        fs::write(path, serialize_note(&note.note))?;
        Ok(())
    }
}

fn serialize_note(note: &Note) -> String {
    let mut tags: Vec<&str> = note.tags.iter().map(String::as_str).collect();
    tags.sort();
    format!("{}\n\n{}", tags.join("\n"), note.contents)
}

fn parse_note(raw: &str) -> Result<Note> {
    let (tag_block, contents) = raw.split_once("\n\n").ok_or_else(|| {
        TiroError::Parse("missing tag/content separator".to_string())
    })?;
    let tags: HashSet<String> = tag_block
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    Ok(Note::new(contents.to_string(), tags))
}
