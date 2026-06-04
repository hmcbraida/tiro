use std::sync::{Arc, Mutex};

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::engine::TiroEngine;
use crate::store::NoteStore;

use super::super::input::wrap;
use super::super::state::NoteViewState;
use super::super::theme;
use super::widgets::tag_span;

pub fn render<S: NoteStore>(
    frame: &mut Frame,
    area: Rect,
    nv: &mut NoteViewState,
    engine: &Arc<Mutex<TiroEngine<S>>>,
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
    engine: &Arc<Mutex<TiroEngine<S>>>,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .title(Span::styled(" tags ", theme::dim()));

    let eng = engine.lock().expect("engine mutex");
    let mut names: Vec<String> =
        eng.get_tags().map(|t| t.name.clone()).collect();
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
        spans.push(tag_span(name, &eng, active));
    }
    let line = Line::from(spans);
    frame.render_widget(Paragraph::new(line).block(block), area);
}

fn render_body(frame: &mut Frame, area: Rect, nv: &mut NoteViewState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        nv.last_view_width = inner.width;
        return;
    }
    nv.last_view_width = inner.width;

    let buffer_lines = nv.buffer.lines();
    let (visual_lines, mapping) =
        wrap::wrap_lines(buffer_lines, inner.width as usize);

    let (row, col) = nv.buffer.cursor();
    let (cursor_vrow, cursor_vcol) =
        wrap::buffer_to_visual(&mapping, buffer_lines, row, col);

    nv.scroll_offset = clamp_scroll(
        nv.scroll_offset,
        cursor_vrow as u16,
        inner.height,
        mapping.total_visual_rows as u16,
    );

    let lines: Vec<Line> = visual_lines
        .iter()
        .map(|chars| Line::from(chars.iter().collect::<String>()))
        .collect();
    let para = Paragraph::new(lines).scroll((nv.scroll_offset, 0));
    frame.render_widget(para, inner);

    let on_screen_vrow = (cursor_vrow as u16).saturating_sub(nv.scroll_offset);
    if on_screen_vrow < inner.height && (cursor_vcol as u16) < inner.width {
        let cx = inner.x.saturating_add(cursor_vcol as u16);
        let cy = inner.y.saturating_add(on_screen_vrow);
        frame.set_cursor_position((cx, cy));
    }
}

fn clamp_scroll(
    current: u16,
    cursor_vrow: u16,
    height: u16,
    total_vrows: u16,
) -> u16 {
    let max_first = total_vrows.saturating_sub(1);
    let mut offset = current.min(max_first);
    if cursor_vrow < offset {
        offset = cursor_vrow;
    } else if cursor_vrow >= offset.saturating_add(height) {
        offset = cursor_vrow.saturating_sub(height.saturating_sub(1));
    }
    offset
}
