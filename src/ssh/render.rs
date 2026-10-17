use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use unicode_segmentation::UnicodeSegmentation;

use super::{
    backend::LinkStatus,
    qr::{QrSizeError, encode_cells},
    state::SshState,
};

const BACKGROUND: Color = Color::Rgb(13, 20, 29);
const PANEL: Color = Color::Rgb(22, 32, 44);
const SELECTED: Color = Color::Rgb(35, 65, 69);
const TEXT: Color = Color::Rgb(230, 237, 241);
const MUTED: Color = Color::Rgb(145, 161, 174);
const ACCENT: Color = Color::Rgb(102, 224, 195);
const OUTGOING: Color = Color::Rgb(137, 187, 255);
const WARNING: Color = Color::Rgb(255, 202, 122);
const ERROR: Color = Color::Rgb(255, 137, 143);

pub fn draw(frame: &mut Frame, state: &SshState) {
    let area = frame.area();
    if let Some(payload) = state.pairing_qr.as_deref() {
        draw_qr(frame, payload, area);
        return;
    }
    frame.render_widget(
        Block::default().style(Style::default().bg(BACKGROUND)),
        area,
    );
    let height = area.height;
    let header_height = if height >= 9 { 3 } else { 2 };
    let footer_height = if height >= 15 {
        5
    } else if height >= 8 {
        3
    } else {
        2
    };
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_height.min(height)),
            Constraint::Min(0),
            Constraint::Length(footer_height.min(height.saturating_sub(header_height))),
        ])
        .split(area);
    draw_header(frame, state, sections[0]);
    draw_conversation(frame, state, sections[1]);
    draw_footer(frame, state, sections[2]);
    if state.show_help {
        draw_help(frame, area);
    }
}

fn draw_header(frame: &mut Frame, state: &SshState, area: Rect) {
    let (status, color) = match &state.link {
        LinkStatus::Starting => ("CONNECTING", WARNING),
        LinkStatus::Pairing => ("PAIRING", WARNING),
        LinkStatus::Connected => ("ONLINE", ACCENT),
        LinkStatus::Disconnected => ("OFFLINE", WARNING),
        LinkStatus::LoggedOut => ("LOGGED OUT", ERROR),
        LinkStatus::Error(_) => ("ERROR", ERROR),
    };
    let mut lines = vec![Line::from(vec![
        Span::styled("  ▣ ZAPFAST", Style::default().fg(ACCENT).bold()),
        Span::styled("  /  SSH", Style::default().fg(MUTED)),
        Span::styled(format!("    ● {status}"), Style::default().fg(color).bold()),
    ])];
    if area.height > 1 && area.width < 65 && area.height > 2 {
        let name_width = area.width.saturating_sub(12) as usize;
        lines.push(Line::from(vec![
            Span::styled("  FROM  ", Style::default().fg(MUTED)),
            Span::styled(
                shorten(&state.from.name, name_width),
                Style::default().fg(TEXT).bold(),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  TO    ", Style::default().fg(MUTED)),
            Span::styled(
                shorten(&state.to.name, name_width),
                Style::default().fg(TEXT).bold(),
            ),
        ]));
    } else if area.height > 1 && area.height < 3 {
        let name_width = area.width.saturating_sub(18) as usize / 2;
        lines.push(Line::from(vec![
            Span::styled("  FROM ", Style::default().fg(MUTED)),
            Span::styled(
                shorten(&state.from.name, name_width),
                Style::default().fg(TEXT),
            ),
            Span::styled(" → TO ", Style::default().fg(MUTED)),
            Span::styled(
                shorten(&state.to.name, name_width),
                Style::default().fg(TEXT),
            ),
        ]));
    } else if area.height > 1 {
        lines.push(Line::from(vec![
            Span::styled("  RECEIVE  ", Style::default().fg(MUTED)),
            Span::styled(
                format!("{} (from)", state.from.name),
                Style::default().fg(TEXT).bold(),
            ),
            Span::styled("   →   SEND  ", Style::default().fg(MUTED)),
            Span::styled(
                format!("{} (to)", state.to.name),
                Style::default().fg(TEXT).bold(),
            ),
        ]));
    }
    if area.height > 2 && area.width >= 65 {
        lines.push(Line::from(vec![
            Span::styled("  ?", Style::default().fg(ACCENT).bold()),
            Span::styled(" help   ", Style::default().fg(MUTED)),
            Span::styled("i", Style::default().fg(ACCENT).bold()),
            Span::styled(" write   ", Style::default().fg(MUTED)),
            Span::styled("/", Style::default().fg(ACCENT).bold()),
            Span::styled(" search   ", Style::default().fg(MUTED)),
            Span::styled("q", Style::default().fg(ACCENT).bold()),
            Span::styled(" quit", Style::default().fg(MUTED)),
        ]));
    }
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(PANEL)),
        area,
    );
}

fn shorten(text: &str, max_width: usize) -> String {
    if display_width(text) <= max_width {
        return text.to_owned();
    }
    let mut shortened = String::new();
    for grapheme in text.graphemes(true) {
        if display_width(&shortened) + display_width(grapheme) + 1 > max_width {
            break;
        }
        shortened.push_str(grapheme);
    }
    shortened.push('…');
    shortened
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
    if area.height == 0 || area.width == 0 {
        return;
    }
    let transcript = state.active_messages();
    let width = area.width.saturating_sub(2).max(1);
    let bubble_width = width.min(72).min((width * 3 / 4).max(20));
    let mut lines = Vec::new();
    let mut selected_y = None;
    for message in transcript.iter().rev() {
        let who = if message.from_me {
            "You"
        } else {
            message.sender_name.as_deref().unwrap_or("Them")
        };
        let selected = state.selected_message.as_deref() == Some(&message.id);
        if selected {
            selected_y = Some(lines.len().min(u16::MAX as usize) as u16);
        }
        let accent = if message.from_me { OUTGOING } else { ACCENT };
        let marker = if selected { "◆" } else { "●" };
        let heading = format!("{marker} {who}  {}", clock(message.timestamp));
        lines.push(message_line(
            &heading,
            width,
            message.from_me,
            selected,
            accent,
            true,
        ));
        for body in wrap_words(
            &message.content.summary(),
            bubble_width.saturating_sub(3).max(1),
        ) {
            lines.push(message_line(
                &format!("│ {body}"),
                width,
                message.from_me,
                selected,
                accent,
                false,
            ));
        }
        lines.push(Line::raw(""));
    }
    if lines.is_empty() {
        let notice = if state.search_query.is_empty() {
            "No messages yet  ·  press i to write"
        } else {
            "No matching messages  ·  Esc clears search"
        };
        lines.push(Line::from(Span::styled(notice, Style::default().fg(MUTED))));
    }
    let title = if state.search_query.is_empty() {
        format!(" CONVERSATION  ·  {} messages ", transcript.len())
    } else {
        format!(" SEARCH RESULTS  ·  {} matches ", transcript.len())
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(MUTED))
        .style(Style::default().bg(BACKGROUND).fg(TEXT));
    let inner_height = area.height.saturating_sub(2);
    let total_height = lines.len().min(u16::MAX as usize) as u16;
    let scroll = selected_y
        .map(|content_y| {
            content_y
                .saturating_sub(inner_height / 3)
                .min(total_height.saturating_sub(inner_height))
        })
        .unwrap_or_else(|| total_height.saturating_sub(inner_height));
    frame.render_widget(Paragraph::new(lines).block(block).scroll((scroll, 0)), area);
}

fn message_line(
    text: &str,
    width: u16,
    own: bool,
    selected: bool,
    accent: Color,
    heading: bool,
) -> Line<'static> {
    let width = usize::from(width);
    let text_width = display_width(text).min(width);
    let indent = if own {
        width.saturating_sub(text_width + 1)
    } else {
        1
    };
    let fg = if heading { accent } else { TEXT };
    let bg = if selected { SELECTED } else { BACKGROUND };
    let style = Style::default().fg(fg).bg(bg);
    Line::from(vec![
        Span::styled(" ".repeat(indent), Style::default().bg(bg)),
        Span::styled(text.to_owned(), if heading { style.bold() } else { style }),
        Span::styled(
            " ".repeat(width.saturating_sub(indent + text_width)),
            Style::default().bg(bg),
        ),
    ])
}

fn display_width(text: &str) -> usize {
    Line::raw(text).width()
}

fn clock(unix_seconds: i64) -> String {
    jiff::Timestamp::from_second(unix_seconds)
        .ok()
        .map(|timestamp| {
            timestamp
                .to_zoned(jiff::tz::TimeZone::system())
                .strftime("%H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

fn wrap_words(text: &str, width: u16) -> Vec<String> {
    let width = usize::from(width.max(1));
    let mut lines = Vec::new();
    for paragraph in text.replace('\t', "    ").split('\n') {
        let mut line = String::new();
        for word in paragraph.split_word_bounds() {
            if display_width(&line) + display_width(word) > width && !line.is_empty() {
                lines.push(line.trim_end().to_owned());
                line.clear();
            }
            for grapheme in word.graphemes(true) {
                if display_width(&line) + display_width(grapheme) > width && !line.is_empty() {
                    lines.push(line.trim_end().to_owned());
                    line.clear();
                }
                if !line.is_empty() || !grapheme.trim().is_empty() {
                    line.push_str(grapheme);
                }
            }
        }
        lines.push(line);
    }
    lines
}

fn draw_footer(frame: &mut Frame, state: &SshState, area: Rect) {
    if area.height == 0 {
        return;
    }
    let title = if state.search_active {
        " SEARCH "
    } else {
        " MESSAGE "
    };
    let hint = if state.search_active {
        "Enter find  ·  Esc cancel"
    } else if state.compose_active {
        "Enter send  ·  Esc cancel"
    } else {
        "i write  ·  / search  ·  j/k select  ·  r reply  ·  PgUp older"
    };
    let context = if let Some(reason) = &state.refusal {
        format!("⚠ {reason}  ·  Esc clears reply")
    } else if let Some(quote) = &state.quoting {
        let preview = state
            .messages
            .iter()
            .find(|message| message.id == quote.id)
            .map(|message| message.content.summary())
            .unwrap_or_else(|| "selected message".into());
        format!("↳ Replying to {preview}  ·  Esc cancels")
    } else if !state.search_query.is_empty() && !state.search_active {
        format!("Search: {}  ·  Esc clears", state.search_query)
    } else {
        hint.to_owned()
    };
    let input = if state.search_active {
        &state.search_query
    } else {
        &state.draft
    };
    let placeholder = if state.search_active {
        "Search this chat…"
    } else {
        "Write a message…"
    };
    let mut lines = Vec::new();
    if area.height >= 5 {
        lines.push(Line::from(Span::styled(
            format!(" {context}"),
            Style::default().fg(if state.refusal.is_some() {
                ERROR
            } else {
                MUTED
            }),
        )));
    }
    if area.height < 5 && state.refusal.is_some() {
        lines.push(Line::from(Span::styled(
            format!(" {context}"),
            Style::default().fg(ERROR),
        )));
    } else {
        lines.push(Line::from(vec![
            Span::styled(" › ", Style::default().fg(ACCENT).bold()),
            Span::styled(
                if input.is_empty() { placeholder } else { input },
                Style::default().fg(if input.is_empty() { MUTED } else { TEXT }),
            ),
            Span::styled(
                if state.search_active || state.compose_active {
                    "▌"
                } else {
                    ""
                },
                Style::default().fg(ACCENT),
            ),
        ]));
    }
    if area.height >= 5 && context != hint {
        lines.push(Line::from(Span::styled(
            format!(" {hint}"),
            Style::default().fg(MUTED),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(
                    Style::default().fg(if state.compose_active || state.search_active {
                        ACCENT
                    } else {
                        MUTED
                    }),
                )
                .style(Style::default().bg(PANEL)),
        ),
        area,
    );
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let width = area.width.min(54).saturating_sub(2).max(1);
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
            Line::raw(""),
            Line::raw("  j / k       Select message"),
            Line::raw("  r           Reply to selection"),
            Line::raw("  i           Write a message"),
            Line::raw("  /           Search this chat"),
            Line::raw("  PgUp        Load older messages"),
            Line::raw("  Esc         Cancel or clear selection"),
            Line::raw("  q           Quit"),
            Line::raw(""),
            Line::raw("  Press ? or Esc to close"),
        ])
        .style(Style::default().bg(PANEL).fg(TEXT))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" ZAPFAST  ·  KEYBOARD ")
                .border_style(Style::default().fg(ACCENT)),
        )
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

    #[test]
    fn narrow_header_keeps_both_directions_visible() {
        let state = SshState::new(
            ResolvedChat {
                id: "to".into(),
                name: "Alice".into(),
                is_group: false,
            },
            ResolvedChat {
                id: "from".into(),
                name: "Team".into(),
                is_group: true,
            },
        );
        let screen = render(&state, 40, 10);
        assert!(screen.contains("FROM  Team"));
        assert!(screen.contains("TO    Alice"));
    }

    #[test]
    fn compact_footer_shows_send_error() {
        let mut state = SshState::default();
        state.refusal = Some("quoted message unavailable".into());
        assert!(render(&state, 40, 10).contains("quoted message unavailable"));
    }
}
