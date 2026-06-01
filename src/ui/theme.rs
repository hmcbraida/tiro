use ratatui::style::{Color, Modifier, Style};

use crate::note::{Tag, TagColor};

pub fn tag_color(tag: &Tag) -> Color {
    match tag.color {
        TagColor::Red => Color::Red,
        TagColor::Green => Color::Green,
        TagColor::Yellow => Color::Yellow,
        TagColor::Blue => Color::Blue,
        TagColor::Magenta => Color::Magenta,
        TagColor::Cyan => Color::Cyan,
        TagColor::LightRed => Color::LightRed,
        TagColor::LightGreen => Color::LightGreen,
        TagColor::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

pub fn tag_chip(tag: &Tag, active: bool) -> Style {
    let style = Style::default().fg(tag_color(tag));
    if active {
        style.add_modifier(Modifier::BOLD)
    } else {
        style.add_modifier(Modifier::DIM)
    }
}

pub fn border() -> Style {
    Style::default().fg(Color::DarkGray)
}

pub fn dim() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}

pub fn placeholder() -> Style {
    Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::ITALIC)
}

pub fn highlight() -> Style {
    Style::default().add_modifier(Modifier::REVERSED)
}

pub fn error_style() -> Style {
    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
}
