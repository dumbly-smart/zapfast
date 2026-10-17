use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
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
    let height = area.height;
    let footer_height = if height < 8 {
        1
    } else if height < 13 {
        2
    } else {
        4
    };
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1.min(height)),
            Constraint::Min(1),
            Constraint::Length(footer_height.min(height.saturating_sub(1))),
        ])
        .split(area);
    let status = format!(
        " {:?} · from {} · to {} · ? help · q quit",
        state.link, state.from.name, state.to.name
    );
    frame.render_widget(Paragraph::new(status), sections[0]);

    draw_conversation(frame, state, sections[1]);

    let footer = if state.search_active {
        format!(" Search: {}", state.search_query)
    } else if state.compose_active {
        format!(" Type message · Enter send · Esc cancel\n{}", state.draft)
    } else if let Some(reason) = &state.refusal {
        format!(" {reason}\n{}", state.draft)
    } else if !state.search_query.is_empty() {
        format!(" Search: {} · Esc clear", state.search_query)
    } else if let Some(quote) = &state.quoting {
        format!(" Reply to {} · Enter compose", quote.id)
    } else {
        " i type · / search · j/k navigate · r reply to selection".to_owned()
    };
    frame.render_widget(
        Paragraph::new(footer).block(Block::default().borders(Borders::ALL).title("Input")),
        sections[2],
    );
    if state.show_help {
        draw_help(frame, area);
    }
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
    let transcript = state.active_messages();
    let width = area.width.saturating_sub(2).max(1);
    let mut lines = Vec::new();
    let mut selected_y = None;
    let mut estimated_height = 0u16;
    for message in transcript.iter().rev() {
        let who = if message.from_me {
            "You"
        } else {
            message.sender_name.as_deref().unwrap_or("Them")
        };
        let selected = state.selected_message.as_deref() == Some(&message.id);
        if selected {
            selected_y = Some(estimated_height);
        }
        let marker = if selected { "› " } else { "  " };
        let prefix = format!("{marker}{who}: ");
        let body = message.content.summary();
        estimated_height =
            estimated_height.saturating_add(wrapped_lines(&format!("{prefix}{body}"), width));
        lines.push(Line::from(vec![
            Span::styled(prefix, Style::default().fg(Color::Yellow)),
            Span::raw(body),
        ]));
        lines.push(Line::raw(""));
        estimated_height = estimated_height.saturating_add(1);
    }
    let title = format!("{} (from) · {} (to)", state.from.name, state.to.name);
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner_height = area.height.saturating_sub(2);
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(block);
    let total_height = estimated_height;
    let scroll = selected_y
        .map(|content_y| {
            // Keep the selected row visible, centering when there is room.
            content_y
                .saturating_sub(inner_height / 2)
                .min(total_height.saturating_sub(inner_height))
        })
        .unwrap_or_else(|| total_height.saturating_sub(inner_height));
    frame.render_widget(paragraph.scroll((scroll, 0)), area);
}

fn wrapped_lines(text: &str, width: u16) -> u16 {
    let width = usize::from(width.max(1));
    text.split('\n')
        .map(|line| line.chars().count().max(1).div_ceil(width) as u16)
        .sum::<u16>()
        .max(1)
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let width = area.width.min(64).saturating_sub(2).max(1);
    let height = area.height.min(13).saturating_sub(2).max(1);
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(vec![
            Line::raw("Keyboard shortcuts"),
            Line::raw("j / k   move through messages"),
            Line::raw("r       reply to selected message"),
            Line::raw("i       compose and send"),
            Line::raw("/       search this chat"),
            Line::raw("Esc     cancel / clear selection"),
            Line::raw("PgUp    load older messages"),
            Line::raw("?       close help     q quit"),
        ])
        .block(Block::default().borders(Borders::ALL).title("Help"))
        .wrap(Wrap { trim: true }),
        popup,
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
