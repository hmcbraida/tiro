use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

use crate::engine::TiroEngine;
use crate::note::StoredNote;
use crate::store::NoteStore;

use super::super::theme;

pub fn tag_span<'a, S: NoteStore>(
    name: &'a str,
    engine: &TiroEngine<S>,
    active: bool,
) -> Span<'a> {
    let style = match engine.get_tag(name) {
        Some(tag) => theme::tag_chip(tag, active),
        None => {
            let s = Style::default().fg(Color::White);
            if active {
                s.add_modifier(Modifier::BOLD)
            } else {
                s.add_modifier(Modifier::DIM)
            }
        }
    };
    Span::styled(name, style)
}

pub fn preview(note: &StoredNote, max: usize) -> String {
    let collapsed: String =
        note.note.contents.lines().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim();
    if trimmed.chars().count() > max {
        let cut: String = trimmed.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    } else {
        trimmed.to_string()
    }
}

pub fn date_from_id(id: &str) -> String {
    let nanos: u128 = id.parse().unwrap_or(0);
    let secs = (nanos / 1_000_000_000) as i64;
    let (y, m, d) = unix_to_ymd(secs);
    format!("{y:04}-{m:02}-{d:02}")
}

fn unix_to_ymd(secs: i64) -> (i64, i64, i64) {
    // Howard Hinnant's date algorithm (proleptic Gregorian).
    let days = secs.div_euclid(86400) + 719468;
    let era = days.div_euclid(146097);
    let doe = days.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 {
        y += 1;
    }
    (y, m, d)
}
