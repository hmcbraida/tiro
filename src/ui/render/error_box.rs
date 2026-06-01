use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use super::super::state::ErrorBox;
use super::super::theme;

pub fn render(frame: &mut Frame, area: Rect, err: &ErrorBox) {
    let w: u16 = 40.min(area.width.saturating_sub(2));
    let lines = ((err.message.len() as u16 / w.max(1)) + 1).min(5);
    let h: u16 = (lines + 2).min(area.height);
    if w == 0 || h == 0 {
        return;
    }
    let x = area.x + area.width.saturating_sub(w + 1);
    let y = area.y + area.height.saturating_sub(h + 1);
    let rect = Rect {
        x,
        y,
        width: w,
        height: h,
    };

    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::error_style())
        .title(Span::styled(" error ", theme::error_style()));
    let p = Paragraph::new(Line::from(Span::styled(
        err.message.as_str(),
        theme::error_style(),
    )))
    .block(block)
    .wrap(Wrap { trim: false });
    frame.render_widget(p, rect);
}
