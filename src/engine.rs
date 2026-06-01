use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    error::{Result, TiroError},
    filter::Filter,
    note::{Note, StoredNote, Tag},
    store::NoteStore,
};

const PAGE_SIZE: usize = 20;

pub struct TiroEngine<S>
where
    S: NoteStore,
{
    note_store: S,
    tags: HashMap<String, Tag>,
}

impl<S: NoteStore> TiroEngine<S> {
    pub fn new(note_store: S, tags: Vec<Tag>) -> Self {
        let tags = tags.into_iter().map(|t| (t.name.clone(), t)).collect();
        Self { note_store, tags }
    }

    pub fn get_tag(&self, name: &str) -> Option<&Tag> {
        self.tags.get(name)
    }

    pub fn get_tags(&self) -> impl Iterator<Item = &Tag> {
        self.tags.values()
    }

    pub fn register_tag(&mut self, tag: Tag) {
        self.tags.insert(tag.name.clone(), tag);
    }

    pub fn get_notes_page(
        &self,
        filter: &Filter,
        page_n: usize,
    ) -> Result<Vec<StoredNote>> {
        let all = self.note_store.list_all_notes()?;
        let start = page_n.saturating_mul(PAGE_SIZE);
        Ok(all
            .into_iter()
            .filter(|n| filter.matches(n))
            .skip(start)
            .take(PAGE_SIZE)
            .collect())
    }

    /// Convenience search bypassing paging; used by the agent's
    /// `search_notes` tool.
    pub fn search(
        &self,
        filter: &Filter,
        limit: usize,
    ) -> Result<Vec<StoredNote>> {
        let all = self.note_store.list_all_notes()?;
        Ok(all
            .into_iter()
            .filter(|n| filter.matches(n))
            .take(limit)
            .collect())
    }

    pub fn get_note(&self, id: &str) -> Result<StoredNote> {
        self.note_store
            .list_all_notes()?
            .into_iter()
            .find(|n| n.id == id)
            .ok_or_else(|| TiroError::NotFound(id.to_string()))
    }

    pub fn create_new_note(&mut self, note: Note) -> Result<StoredNote> {
        let stored = StoredNote::new(generate_id(), note);
        self.note_store.put_note(stored.clone())?;
        Ok(stored)
    }

    pub fn update_note_contents(
        &mut self,
        id: &str,
        contents: String,
    ) -> Result<()> {
        let mut stored = self.get_note(id)?;
        stored.note.contents = contents;
        self.note_store.put_note(stored)
    }

    pub fn update_note_tags(
        &mut self,
        id: &str,
        tags: Vec<String>,
    ) -> Result<()> {
        let mut stored = self.get_note(id)?;
        stored.note.tags = tags.into_iter().collect();
        self.note_store.put_note(stored)
    }
}

pub fn generate_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:032}")
}
