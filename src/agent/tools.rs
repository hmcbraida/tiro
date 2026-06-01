//! Tool surface exposed to the LLM. Each tool's dispatch fn locks the
//! shared engine and returns a JSON value (or error). The model decides
//! whether to retry on tool errors.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::engine::TiroEngine;
use crate::filter::Filter;
use crate::note::{Note, StoredNote};
use crate::store::NoteStore;

const SEARCH_LIMIT: usize = 50;

/// A tool's static description, suitable for handing to the model.
#[derive(Debug, Clone, Serialize)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
}

/// All tools the agent can invoke. Construct via [`all_specs`].
pub fn all_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "search_notes",
            description: "Search notes by free-form query. Supports `tag:NAME` and `-tag:NAME` tokens.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"],
            }),
        },
        ToolSpec {
            name: "get_note",
            description: "Fetch a single note by id.",
            parameters: json!({
                "type": "object",
                "properties": { "id": { "type": "string" } },
                "required": ["id"],
            }),
        },
        ToolSpec {
            name: "list_tags",
            description: "List the user's registered tags.",
            parameters: json!({
                "type": "object",
                "properties": {},
            }),
        },
        ToolSpec {
            name: "create_note",
            description: "Create a new note. Unknown tags are auto-registered.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "contents": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["contents", "tags"],
            }),
        },
        ToolSpec {
            name: "update_note_contents",
            description: "Replace a note's contents.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "contents": { "type": "string" }
                },
                "required": ["id", "contents"],
            }),
        },
        ToolSpec {
            name: "update_note_tags",
            description: "Replace a note's tags. Unknown tags are auto-registered.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["id", "tags"],
            }),
        },
    ]
}

/// Dispatch a single tool call. Always returns a JSON value; the second
/// tuple element is true if the model should treat the result as an error.
pub fn dispatch<S: NoteStore>(
    engine: &Arc<Mutex<TiroEngine<S>>>,
    name: &str,
    args: &Value,
) -> (Value, bool) {
    match name {
        "search_notes" => match args.get("query").and_then(|v| v.as_str()) {
            Some(q) => {
                let filter = Filter::parse(q);
                let eng = engine.lock().expect("engine mutex poisoned");
                match eng.search(&filter, SEARCH_LIMIT) {
                    Ok(rows) => (
                        json!(rows.iter().map(note_value).collect::<Vec<_>>()),
                        false,
                    ),
                    Err(e) => (json!({ "error": e.to_string() }), true),
                }
            }
            None => (json!({ "error": "missing string 'query'" }), true),
        },
        "get_note" => match args.get("id").and_then(|v| v.as_str()) {
            Some(id) => {
                let eng = engine.lock().expect("engine mutex poisoned");
                match eng.get_note(id) {
                    Ok(n) => (note_value(&n), false),
                    Err(e) => (json!({ "error": e.to_string() }), true),
                }
            }
            None => (json!({ "error": "missing string 'id'" }), true),
        },
        "list_tags" => {
            let eng = engine.lock().expect("engine mutex poisoned");
            let names: Vec<String> =
                eng.get_tags().map(|t| t.name.clone()).collect();
            (
                json!(
                    names
                        .iter()
                        .map(|n| json!({ "name": n }))
                        .collect::<Vec<_>>()
                ),
                false,
            )
        }
        "create_note" => {
            let contents =
                args.get("contents").and_then(|v| v.as_str()).unwrap_or("");
            let tags = args
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let mut eng = engine.lock().expect("engine mutex poisoned");
            register_unknown_tags(&mut eng, &tags);
            let set: HashSet<String> = tags.into_iter().collect();
            match eng.create_new_note(Note::new(contents.to_string(), set)) {
                Ok(stored) => (json!({ "id": stored.id }), false),
                Err(e) => (json!({ "error": e.to_string() }), true),
            }
        }
        "update_note_contents" => {
            let Some(id) = args.get("id").and_then(|v| v.as_str()) else {
                return (json!({ "error": "missing 'id'" }), true);
            };
            let contents =
                args.get("contents").and_then(|v| v.as_str()).unwrap_or("");
            let mut eng = engine.lock().expect("engine mutex poisoned");
            match eng.update_note_contents(id, contents.to_string()) {
                Ok(()) => (json!({}), false),
                Err(e) => (json!({ "error": e.to_string() }), true),
            }
        }
        "update_note_tags" => {
            let Some(id) = args.get("id").and_then(|v| v.as_str()) else {
                return (json!({ "error": "missing 'id'" }), true);
            };
            let tags = args
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let mut eng = engine.lock().expect("engine mutex poisoned");
            register_unknown_tags(&mut eng, &tags);
            match eng.update_note_tags(id, tags) {
                Ok(()) => (json!({}), false),
                Err(e) => (json!({ "error": e.to_string() }), true),
            }
        }
        other => (json!({ "error": format!("unknown tool '{other}'") }), true),
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

// `serde::Deserialize` only used downstream by other modules consuming the
// tool args; kept here for clarity in case we want strongly-typed args.
#[derive(Deserialize)]
#[allow(dead_code)]
struct SearchArgs {
    query: String,
}
