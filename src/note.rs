use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone)]
pub struct Note {
    pub contents: String,
    pub tags: HashSet<String>,
}

impl Note {
    pub fn new(contents: String, tags: HashSet<String>) -> Self {
        Self { contents, tags }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagColor {
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    LightRed,
    LightGreen,
}

impl TagColor {
    pub const PALETTE: [TagColor; 8] = [
        TagColor::Red,
        TagColor::Green,
        TagColor::Yellow,
        TagColor::Blue,
        TagColor::Magenta,
        TagColor::Cyan,
        TagColor::LightRed,
        TagColor::LightGreen,
    ];
}

#[derive(Debug, Clone)]
pub struct Tag {
    pub name: String,
    pub color: TagColor,
}

impl Tag {
    pub fn new(name: String) -> Self {
        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        let idx = (hasher.finish() as usize) % TagColor::PALETTE.len();
        Self {
            name,
            color: TagColor::PALETTE[idx],
        }
    }
}

#[derive(Debug, Clone)]
pub struct StoredNote {
    pub id: String,
    pub note: Note,
}

impl StoredNote {
    pub fn new(id: String, note: Note) -> Self {
        Self { id, note }
    }
}
