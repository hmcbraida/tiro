use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, ListState, Paragraph,
};

use crate::engine::TiroEngine;
use crate::store::NoteStore;

use super::super::state::TagPickerState;
use super::super::theme;
use super::super::update::visible_tags;
use super::widgets::tag_span;

pub fn render<S: NoteStore>(
    frame: &mut Frame,
    area: Rect,
    tp: &TagPickerState,
    engine: &TiroEngine<S>,
) {
    super::note_view::render(frame, area, &tp.origin, engine);

    let popup = centered(area, 50, 60);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .title(Span::styled(" tags ", theme::dim()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    let filter_text = if tp.filter.is_empty() {
        Line::from(Span::styled("filter…", theme::placeholder()))
    } else {
        Line::from(Span::raw(tp.filter.text()))
    };
    frame.render_widget(Paragraph::new(filter_text), chunks[0]);
    frame.set_cursor_position((
        chunks[0].x + tp.filter.cursor() as u16,
        chunks[0].y,
    ));

    let names = visible_tags(tp, engine);
    let items: Vec<ListItem> = names
        .iter()
        .map(|name| {
            let mark = if tp.pending.contains(name) {
                "[x] "
            } else {
                "[ ] "
            };
            let line = Line::from(vec![
                Span::raw(mark),
                tag_span(name, engine, tp.pending.contains(name)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(items).highlight_style(theme::highlight());
    let mut list_state = ListState::default();
    if !names.is_empty() {
        list_state.select(Some(tp.cursor.min(names.len() - 1)));
    }
    frame.render_stateful_widget(list, chunks[1], &mut list_state);
}

fn centered(area: Rect, pct_x: u16, pct_y: u16) -> Rect {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - pct_y) / 2),
            Constraint::Percentage(pct_y),
            Constraint::Percentage((100 - pct_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - pct_x) / 2),
            Constraint::Percentage(pct_x),
            Constraint::Percentage((100 - pct_x) / 2),
        ])
        .split(vert[1])[1]
}
