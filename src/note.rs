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
    Rgb(u8, u8, u8),
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

    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.strip_prefix('#').unwrap_or(s);
        if s.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some(TagColor::Rgb(r, g, b))
    }

    pub fn hashed_from_name(name: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        let idx = (hasher.finish() as usize) % Self::PALETTE.len();
        Self::PALETTE[idx]
    }
}

#[derive(Debug, Clone)]
pub struct Tag {
    pub name: String,
    pub color: TagColor,
}

impl Tag {
    pub fn new(name: String, color: Option<TagColor>) -> Self {
        let color = color.unwrap_or_else(|| TagColor::hashed_from_name(&name));
        Self { name, color }
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
