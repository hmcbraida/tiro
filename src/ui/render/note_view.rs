use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::engine::TiroEngine;
use crate::store::NoteStore;

use super::super::state::NoteViewState;
use super::super::theme;
use super::widgets::tag_span;

pub fn render<S: NoteStore>(
    frame: &mut Frame,
    area: Rect,
    nv: &NoteViewState,
    engine: &TiroEngine<S>,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    render_header(frame, chunks[0], nv, engine);
    render_body(frame, chunks[1], nv);
}

fn render_header<S: NoteStore>(
    frame: &mut Frame,
    area: Rect,
    nv: &NoteViewState,
    engine: &TiroEngine<S>,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .title(Span::styled(" tags ", theme::dim()));
    let mut names: Vec<String> =
        engine.get_tags().map(|t| t.name.clone()).collect();
    names.sort();

    let mut spans: Vec<Span> = Vec::new();
    for (i, name) in names.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }
        let active = nv.tags.contains(name);
        let label = if i < 9 {
            format!("{}:", i + 1)
        } else if i == 9 {
            "0:".to_string()
        } else {
            "  ".to_string()
        };
        spans.push(Span::styled(label, theme::dim()));
        spans.push(tag_span(name, engine, active));
    }
    let line = Line::from(spans);
    frame.render_widget(Paragraph::new(line).block(block), area);
}

fn render_body(frame: &mut Frame, area: Rect, nv: &NoteViewState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line> = nv
        .buffer
        .lines()
        .iter()
        .map(|chars| Line::from(chars.iter().collect::<String>()))
        .collect();
    let para = Paragraph::new(lines);
    frame.render_widget(para, inner);

    let (row, col) = nv.buffer.cursor();
    let cx = inner.x.saturating_add(col as u16);
    let cy = inner.y.saturating_add(row as u16);
    frame.set_cursor_position((cx, cy));
}
