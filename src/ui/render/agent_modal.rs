use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::agent::session::TranscriptMessage;

use super::super::state::AgentModalState;
use super::super::theme;

pub fn render(frame: &mut Frame, area: Rect, m: &AgentModalState) {
    let popup = centered(area, 80, 80);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme::border())
        .title(Span::styled(" agent ", theme::dim()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(inner);

    render_header(frame, chunks[0], m);
    render_transcript(frame, chunks[1], m);
    render_input(frame, chunks[2], m);
}

fn render_header(frame: &mut Frame, area: Rect, m: &AgentModalState) {
    let title = m.session.title();
    let attached = if m.session.attached_note_ids.is_empty() {
        String::new()
    } else {
        format!("  attached: {}", m.session.attached_note_ids.len())
    };
    let header = Line::from(vec![
        Span::styled("session: ", theme::dim()),
        Span::raw(title),
        Span::styled(attached, theme::dim()),
    ]);
    frame.render_widget(Paragraph::new(header), area);
}

fn render_transcript(frame: &mut Frame, area: Rect, m: &AgentModalState) {
    let block = Block::default()
        .borders(Borders::TOP | Borders::BOTTOM)
        .border_style(theme::border());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    for msg in &m.session.messages {
        match msg {
            TranscriptMessage::User(text) => {
                lines.push(Line::from(Span::styled(
                    "you:",
                    Style::default().add_modifier(Modifier::BOLD),
                )));
                for ln in text.lines() {
                    lines.push(Line::from(Span::raw(ln.to_string())));
                }
                lines.push(Line::from(""));
            }
            TranscriptMessage::Assistant(text) => {
                lines.push(Line::from(Span::styled(
                    "agent:",
                    Style::default().add_modifier(Modifier::BOLD),
                )));
                for ln in text.lines() {
                    lines.push(Line::from(Span::raw(ln.to_string())));
                }
                lines.push(Line::from(""));
            }
            TranscriptMessage::ToolCall { name, args } => {
                let summary = format!(
                    "→ {name}({})",
                    serde_json::to_string(args).unwrap_or_default()
                );
                lines.push(Line::from(Span::styled(summary, theme::dim())));
            }
            TranscriptMessage::ToolResult {
                name,
                result,
                is_error,
            } => {
                let prefix = if *is_error { "← err " } else { "← " };
                let summary =
                    format!("{prefix}{name}: {}", short_value(result));
                lines.push(Line::from(Span::styled(
                    summary,
                    if *is_error {
                        theme::error_style()
                    } else {
                        theme::dim()
                    },
                )));
            }
            TranscriptMessage::SystemError(msg) => {
                lines.push(Line::from(Span::styled(
                    format!("! {msg}"),
                    theme::error_style(),
                )));
            }
        }
    }
    if !m.streaming_text.is_empty() {
        lines.push(Line::from(Span::styled(
            "agent (streaming):",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        for ln in m.streaming_text.lines() {
            lines.push(Line::from(Span::raw(ln.to_string())));
        }
    }

    let para = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((m.scroll, 0));
    frame.render_widget(para, inner);
}

fn render_input(frame: &mut Frame, area: Rect, m: &AgentModalState) {
    if m.in_flight.is_some() {
        let line = Line::from(vec![
            Span::styled("… streaming  ", theme::dim()),
            Span::styled("(Esc to cancel)", theme::placeholder()),
        ]);
        frame.render_widget(Paragraph::new(line), area);
        return;
    }
    // Disabled (no provider) state: input area replaced with hint.
    // We detect "disabled" by the session having no id collisions with a
    // store, but for now we rely on the absence of an in-flight handle and
    // accept input universally; the update layer no-ops submit if runtime
    // is disabled.
    let text = if m.input.is_empty() {
        Line::from(Span::styled(
            "ask the agent…  (Enter to send, Esc to close)",
            theme::placeholder(),
        ))
    } else {
        Line::from(Span::raw(m.input.text()))
    };
    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(theme::border());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(text), inner);
    frame.set_cursor_position((inner.x + m.input.cursor() as u16, inner.y));
}

fn short_value(v: &serde_json::Value) -> String {
    let s = serde_json::to_string(v).unwrap_or_default();
    if s.chars().count() > 80 {
        let head: String = s.chars().take(77).collect();
        format!("{head}…")
    } else {
        s
    }
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
