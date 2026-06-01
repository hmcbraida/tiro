use chrono::{DateTime, Local};

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, ListState, Paragraph,
};

use super::super::state::SessionPickerState;
use super::super::theme;
use super::super::update::visible_sessions;

pub fn render(frame: &mut Frame, area: Rect, sp: &SessionPickerState) {
    let popup = centered(area, 70, 70);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .title(Span::styled(" sessions ", theme::dim()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    let filter_text = if sp.filter.is_empty() {
        Line::from(Span::styled("filter…", theme::placeholder()))
    } else {
        Line::from(Span::raw(sp.filter.text()))
    };
    frame.render_widget(Paragraph::new(filter_text), chunks[0]);
    frame.set_cursor_position((
        chunks[0].x + sp.filter.cursor() as u16,
        chunks[0].y,
    ));

    let visible = visible_sessions(sp);
    let items: Vec<ListItem> = visible
        .iter()
        .map(|&i| {
            let s = &sp.sessions[i];
            let when: DateTime<Local> = s.created_at.into();
            let label =
                format!("{}  --  {}", when.format("%Y-%m-%d %H:%M"), s.title());
            ListItem::new(Line::from(Span::raw(label)))
        })
        .collect();

    let list = List::new(items).highlight_style(theme::highlight());
    let mut list_state = ListState::default();
    if !visible.is_empty() {
        list_state.select(Some(sp.cursor.min(visible.len() - 1)));
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
