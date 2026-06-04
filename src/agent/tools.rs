//! Tool surface exposed to the LLM via rig's [`Tool`] trait. Each tool
//! owns a shared engine handle and serialises its result back to JSON
//! for the model. Engine errors are encoded as `{"error": "..."}` in
//! the success payload so the model can react without us threading a
//! separate error channel through rig.

use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::engine::TiroEngine;
use crate::filter::Filter;
use crate::note::{Note, StoredNote};
use crate::store::NoteStore;

const SEARCH_LIMIT: usize = 50;

pub type EngineHandle<S> = Arc<Mutex<TiroEngine<S>>>;

/// Build the full tool set bound to `engine`. Returned as boxed dyn
/// tools so a single `Vec` can carry them into [`rig::agent::AgentBuilder::tools`].
pub fn build_tools<S>(
    engine: EngineHandle<S>,
) -> Vec<Box<dyn rig::tool::ToolDyn>>
where
    S: NoteStore + Send + 'static,
{
    vec![
        Box::new(SearchNotes::new(engine.clone())),
        Box::new(GetNote::new(engine.clone())),
        Box::new(ListTags::new(engine.clone())),
        Box::new(CreateNote::new(engine.clone())),
        Box::new(UpdateNoteContents::new(engine.clone())),
        Box::new(UpdateNoteTags::new(engine)),
    ]
}

#[derive(Deserialize)]
pub struct SearchArgs {
    pub query: String,
}

pub struct SearchNotes<S: NoteStore> {
    engine: EngineHandle<S>,
}

impl<S: NoteStore> SearchNotes<S> {
    pub fn new(engine: EngineHandle<S>) -> Self {
        Self { engine }
    }
}

impl<S> Tool for SearchNotes<S>
where
    S: NoteStore + Send + 'static,
{
    const NAME: &'static str = "search_notes";
    type Error = Infallible;
    type Args = SearchArgs;
    type Output = Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description:
                "Search notes. Try searching for word roots: so 'dog' not 'dogs'. You can query tags with tag:NAME and -tag:NAME"
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "query": { "type": "string" } },
                "required": ["query"],
            }),
        }
    }

    async fn call(
        &self,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let filter = Filter::parse(&args.query);
        let eng = self.engine.lock().expect("engine mutex poisoned");
        Ok(match eng.search(&filter, SEARCH_LIMIT) {
            Ok(rows) => json!(rows.iter().map(note_value).collect::<Vec<_>>()),
            Err(e) => json!({ "error": e.to_string() }),
        })
    }
}

#[derive(Deserialize)]
pub struct GetNoteArgs {
    pub id: String,
}

pub struct GetNote<S: NoteStore> {
    engine: EngineHandle<S>,
}

impl<S: NoteStore> GetNote<S> {
    pub fn new(engine: EngineHandle<S>) -> Self {
        Self { engine }
    }
}

impl<S> Tool for GetNote<S>
where
    S: NoteStore + Send + 'static,
{
    const NAME: &'static str = "get_note";
    type Error = Infallible;
    type Args = GetNoteArgs;
    type Output = Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Fetch a single note by id.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"],
            }),
        }
    }

    async fn call(
        &self,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let eng = self.engine.lock().expect("engine mutex poisoned");
        Ok(match eng.get_note(&args.id) {
            Ok(n) => note_value(&n),
            Err(e) => json!({ "error": e.to_string() }),
        })
    }
}

#[derive(Deserialize, Default)]
pub struct ListTagsArgs {}

pub struct ListTags<S: NoteStore> {
    engine: EngineHandle<S>,
}

impl<S: NoteStore> ListTags<S> {
    pub fn new(engine: EngineHandle<S>) -> Self {
        Self { engine }
    }
}

impl<S> Tool for ListTags<S>
where
    S: NoteStore + Send + 'static,
{
    const NAME: &'static str = "list_tags";
    type Error = Infallible;
    type Args = ListTagsArgs;
    type Output = Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "List the user's registered tags.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {},
            }),
        }
    }

    async fn call(
        &self,
        _args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let eng = self.engine.lock().expect("engine mutex poisoned");
        let names: Vec<String> =
            eng.get_tags().map(|t| t.name.clone()).collect();
        Ok(json!(
            names
                .iter()
                .map(|n| json!({ "name": n }))
                .collect::<Vec<_>>()
        ))
    }
}

#[derive(Deserialize)]
pub struct CreateNoteArgs {
    pub contents: String,
    pub tags: Vec<String>,
}

pub struct CreateNote<S: NoteStore> {
    engine: EngineHandle<S>,
}

impl<S: NoteStore> CreateNote<S> {
    pub fn new(engine: EngineHandle<S>) -> Self {
        Self { engine }
    }
}

impl<S> Tool for CreateNote<S>
where
    S: NoteStore + Send + 'static,
{
    const NAME: &'static str = "create_note";
    type Error = Infallible;
    type Args = CreateNoteArgs;
    type Output = Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Create a new note. Unknown tags are auto-registered."
                .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "contents": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["contents", "tags"],
            }),
        }
    }

    async fn call(
        &self,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let mut eng = self.engine.lock().expect("engine mutex poisoned");
        register_unknown_tags(&mut eng, &args.tags);
        let set: HashSet<String> = args.tags.into_iter().collect();
        Ok(match eng.create_new_note(Note::new(args.contents, set)) {
            Ok(stored) => json!({ "id": stored.id }),
            Err(e) => json!({ "error": e.to_string() }),
        })
    }
}

#[derive(Deserialize)]
pub struct UpdateNoteContentsArgs {
    pub id: String,
    pub contents: String,
}

pub struct UpdateNoteContents<S: NoteStore> {
    engine: EngineHandle<S>,
}

impl<S: NoteStore> UpdateNoteContents<S> {
    pub fn new(engine: EngineHandle<S>) -> Self {
        Self { engine }
    }
}

impl<S> Tool for UpdateNoteContents<S>
where
    S: NoteStore + Send + 'static,
{
    const NAME: &'static str = "update_note_contents";
    type Error = Infallible;
    type Args = UpdateNoteContentsArgs;
    type Output = Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Replace a note's contents.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "contents": { "type": "string" }
                },
                "required": ["id", "contents"],
            }),
        }
    }

    async fn call(
        &self,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let mut eng = self.engine.lock().expect("engine mutex poisoned");
        Ok(match eng.update_note_contents(&args.id, args.contents) {
            Ok(()) => json!({}),
            Err(e) => json!({ "error": e.to_string() }),
        })
    }
}

#[derive(Deserialize)]
pub struct UpdateNoteTagsArgs {
    pub id: String,
    pub tags: Vec<String>,
}

pub struct UpdateNoteTags<S: NoteStore> {
    engine: EngineHandle<S>,
}

impl<S: NoteStore> UpdateNoteTags<S> {
    pub fn new(engine: EngineHandle<S>) -> Self {
        Self { engine }
    }
}

impl<S> Tool for UpdateNoteTags<S>
where
    S: NoteStore + Send + 'static,
{
    const NAME: &'static str = "update_note_tags";
    type Error = Infallible;
    type Args = UpdateNoteTagsArgs;
    type Output = Value;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description:
                "Replace a note's tags. Unknown tags are auto-registered."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["id", "tags"],
            }),
        }
    }

    async fn call(
        &self,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let mut eng = self.engine.lock().expect("engine mutex poisoned");
        register_unknown_tags(&mut eng, &args.tags);
        Ok(match eng.update_note_tags(&args.id, args.tags) {
            Ok(()) => json!({}),
            Err(e) => json!({ "error": e.to_string() }),
        })
    }
}

fn register_unknown_tags<S: NoteStore>(
    engine: &mut TiroEngine<S>,
    names: &[String],
) {
    for name in names {
        if engine.get_tag(name).is_none() {
            engine.register_tag(crate::note::Tag::new(name.clone(), None));
        }
    }
}

fn note_value(n: &StoredNote) -> Value {
    let mut tags: Vec<&String> = n.note.tags.iter().collect();
    tags.sort();
    json!({
        "id": n.id,
        "contents": n.note.contents,
        "tags": tags,
    })
}
