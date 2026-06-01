use std::sync::{Arc, Mutex};

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::engine::TiroEngine;
use crate::store::NoteStore;

use super::super::state::SearchState;
use super::super::theme;
use super::widgets::{date_from_id, preview, tag_span};

pub fn render<S: NoteStore>(
    frame: &mut Frame,
    area: Rect,
    s: &SearchState,
    engine: &Arc<Mutex<TiroEngine<S>>>,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    render_search_input(frame, chunks[0], s);
    render_results(frame, chunks[1], s, engine);
}

fn render_search_input(frame: &mut Frame, area: Rect, s: &SearchState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .title(Span::styled(" search your notes ", theme::dim()));

    let text = if s.query.is_empty() {
        Line::from(Span::styled("type to filter…", theme::placeholder()))
    } else {
        Line::from(Span::raw(s.query.text()))
    };

    frame.render_widget(Paragraph::new(text).block(block), area);

    let inner_x = area.x + 1;
    let inner_y = area.y + 1;
    let cur_x = inner_x + s.query.cursor() as u16;
    frame.set_cursor_position((cur_x, inner_y));
}

fn render_results<S: NoteStore>(
    frame: &mut Frame,
    area: Rect,
    s: &SearchState,
    engine: &Arc<Mutex<TiroEngine<S>>>,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if s.results.is_empty() {
        let msg = Paragraph::new(Line::from(Span::styled(
            "no notes",
            theme::placeholder(),
        )));
        frame.render_widget(msg, inner);
        return;
    }

    let eng = engine.lock().expect("engine mutex");
    let date_w: usize = 10;
    let body_w = (inner.width as usize).saturating_sub(date_w + 2);

    let items: Vec<ListItem> = s
        .results
        .iter()
        .map(|note| {
            let mut spans: Vec<Span> = Vec::new();
            let preview_max = body_w.saturating_sub(20).max(10);
            let prev = preview(note, preview_max);
            spans.push(Span::raw(prev.clone()));
            spans.push(Span::styled("  tags: ", theme::dim()));
            let mut tag_names: Vec<&String> = note.note.tags.iter().collect();
            tag_names.sort();
            for (i, name) in tag_names.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::raw(" "));
                }
                spans.push(tag_span(name, &eng, true));
            }

            let left_text_width: usize =
                spans.iter().map(|s| s.content.chars().count()).sum();
            let pad = body_w.saturating_sub(left_text_width);
            if pad > 0 {
                spans.push(Span::raw(" ".repeat(pad)));
            }
            spans.push(Span::styled(date_from_id(&note.id), theme::dim()));

            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items).highlight_style(theme::highlight());
    let mut list_state = ListState::default();
    list_state.select(Some(s.cursor));
    frame.render_stateful_widget(list, inner, &mut list_state);

    let _ = Style::default();
}
