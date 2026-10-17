use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use super::{
    qr::{QrSizeError, encode_cells},
    state::SshState,
};

pub fn draw(frame: &mut Frame, state: &SshState) {
    let area = frame.area();
    if let Some(payload) = state.pairing_qr.as_deref() {
        draw_qr(frame, payload, area);
        return;
    }
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(area);
    let status = format!(
        " ZapFast SSH · {:?} · q quit · / search · i compose · r reply ",
        state.link
    );
    frame.render_widget(Paragraph::new(status), sections[0]);

    draw_conversation(frame, state, sections[1]);

    let footer = if let Some(reason) = &state.refusal {
        format!(" Send refused: {reason}\n{}", state.draft)
    } else if !state.search_query.is_empty() {
        format!(" Search: {}", state.search_query)
    } else if let Some(quote) = &state.quoting {
        format!(" Reply to {}\n{}", quote.id, state.draft)
    } else {
        format!(" {}", state.draft)
    };
    frame.render_widget(
        Paragraph::new(footer).block(Block::default().borders(Borders::ALL).title("Input")),
        sections[2],
    );
}

fn draw_qr(frame: &mut Frame, payload: &str, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Scan with WhatsApp · Linked devices");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    match encode_cells(payload, inner.width, inner.height) {
        Ok(rows) => {
            let text = Text::from(rows.into_iter().map(Line::from).collect::<Vec<_>>());
            let text_height = text.height().min(u16::MAX as usize) as u16;
            let y = inner.y + inner.height.saturating_sub(text_height) / 2;
            let qr_area = Rect::new(inner.x, y, inner.width, text_height);
            frame.render_widget(
                Paragraph::new(text)
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::Black).bg(Color::White)),
                qr_area,
            );
        }
        Err(QrSizeError::TooSmall | QrSizeError::InvalidCode) => {
            frame.render_widget(
                Paragraph::new("Resize terminal to scan this pairing code.")
                    .alignment(Alignment::Center),
                inner,
            );
        }
    }
}

fn draw_conversation(frame: &mut Frame, state: &SshState, area: Rect) {
    let transcript = if state.search_query.is_empty() {
        &state.messages
    } else {
        &state.search_results
    };
    let lines = transcript
        .iter()
        .rev()
        .map(|message| {
            let who = if message.from_me {
                "You"
            } else {
                message.sender_name.as_deref().unwrap_or("Them")
            };
            Line::from(vec![
                Span::styled(format!("{who}: "), Style::default().fg(Color::Yellow)),
                Span::raw(message.content.summary()),
            ])
        })
        .collect::<Vec<_>>();
    let title = format!("{} (from) · {} (to)", state.from.name, state.to.name);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(title)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::{Chat, Content, Delivery, Message},
        ssh::{backend::LinkStatus, selector::ResolvedChat, state::SshState},
    };
    use ratatui::{Terminal, backend::TestBackend};

    fn render(state: &SshState, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, state)).unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn small_terminal_hides_qr_payload() {
        let mut state = SshState::default();
        state.pairing_qr = Some("PRIVATE_PAIRING_FIXTURE".into());
        let screen = render(&state, 30, 8);
        assert!(screen.contains("Resize terminal"));
        assert!(!screen.contains("PRIVATE_PAIRING_FIXTURE"));
    }

    #[test]
    fn pairing_qr_uses_the_full_screen_on_a_typical_terminal() {
        let mut state = SshState::default();
        state.pairing_qr = Some("fixture payload".into());
        let screen = render(&state, 80, 24);
        assert!(!screen.contains("Resize terminal"));
        assert!(screen.contains('▀') || screen.contains('▄') || screen.contains('█'));
    }

    #[test]
    fn unsupported_message_has_placeholder() {
        let mut state = SshState::default();
        state.link = LinkStatus::Connected;
        state.open_chat = Some("chat".into());
        state.chats.push(Chat::new("chat".into(), "chat".into()));
        state.messages.push(Message {
            chat: "chat".into(),
            id: "m1".into(),
            sender: "peer".into(),
            sender_name: None,
            from_me: false,
            timestamp: 1,
            history_order: None,
            content: Content::Unsupported {
                what: "image".into(),
            },
            status: Delivery::Sent,
            delivered_at: None,
            read_at: None,
            quoted: None,
            reactions: vec![],
            edited: false,
            mentions: vec![],
            forwarded: false,
            thumbnail: None,
        });
        assert!(render(&state, 100, 24).contains("Unsupported"));
    }

    #[test]
    fn resize_keeps_selected_chat_id() {
        let mut state = SshState::default();
        state.chats = vec![
            Chat::new("one".into(), "one".into()),
            Chat::new("two".into(), "two".into()),
        ];
        state.selected_chat = 1;
        let _ = render(&state, 80, 20);
        let _ = render(&state, 40, 10);
        assert_eq!(state.visible_chats()[state.selected_chat].id, "two");
    }

    #[test]
    fn direct_mode_header_shows_both_selected_conversations_without_chat_list() {
        let state = SshState::new(
            ResolvedChat {
                id: "to@s.whatsapp.net".into(),
                name: "Send Target".into(),
                is_group: false,
            },
            ResolvedChat {
                id: "team@g.us".into(),
                name: "Design Team".into(),
                is_group: true,
            },
        );
        let screen = render(&state, 100, 24);
        assert!(screen.contains("Design Team (from)"));
        assert!(screen.contains("Send Target (to)"));
        assert!(!screen.contains("Chats"));
    }
}
