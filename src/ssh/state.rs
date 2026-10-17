//! Terminal-facing reducer: views render this state and turn keys into actions.

use crate::model::{Chat, Message, ReplyTarget};

use super::backend::{Command, Event, LinkStatus};
use super::selector::ResolvedChat;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputAction {
    SelectChat(usize),
    ScrollChats(isize),
    LoadMoreChats,
    OpenSelected,
    OpenChat(String),
    Back,
    OlderPage,
    Search(String),
    BeginSearch,
    BeginCompose,
    EndSearch,
    CancelInput,
    ToggleHelp,
    Compose(String),
    SelectMessage(usize),
    QuoteSelected,
    Send,
    Quit,
}

#[derive(Debug)]
pub struct SshState {
    pub to: ResolvedChat,
    pub from: ResolvedChat,
    pub link: LinkStatus,
    pub pairing_qr: Option<String>,
    pub chats: Vec<Chat>,
    pub browse_mode: bool,
    pub chats_exhausted: bool,
    pub selected_chat: usize,
    pub open_chat: Option<String>,
    /// Messages stay in archive order (newest first); the renderer draws them oldest first.
    pub messages: Vec<Message>,
    pub selected_message: Option<String>,
    pub draft: String,
    pub quoting: Option<ReplyTarget>,
    pub refusal: Option<String>,
    pub search_query: String,
    pub search_results: Vec<Message>,
    pub history_complete: bool,
    pub quit: bool,
    pub search_active: bool,
    pub compose_active: bool,
    pub show_help: bool,
    older_loading: bool,
    phone_history_requested: bool,
    local_history_exhausted: bool,
    chat_page_loading: bool,
}

pub const CHAT_PAGE_SIZE: usize = 10;

impl SshState {
    pub fn new(to: ResolvedChat, from: ResolvedChat) -> Self {
        Self {
            open_chat: Some(from.id.clone()),
            to,
            from,
            link: LinkStatus::Starting,
            pairing_qr: None,
            chats: Vec::new(),
            browse_mode: false,
            chats_exhausted: true,
            selected_chat: 0,
            messages: Vec::new(),
            selected_message: None,
            draft: String::new(),
            quoting: None,
            refusal: None,
            search_query: String::new(),
            search_results: Vec::new(),
            history_complete: false,
            quit: false,
            search_active: false,
            compose_active: false,
            show_help: false,
            older_loading: false,
            phone_history_requested: false,
            local_history_exhausted: false,
            chat_page_loading: false,
        }
    }

    pub fn browse() -> Self {
        let placeholder = ResolvedChat {
            id: String::new(),
            name: String::new(),
            is_group: false,
        };
        let mut state = Self::new(placeholder.clone(), placeholder);
        state.open_chat = None;
        state.browse_mode = true;
        state.chats_exhausted = false;
        state
    }

    pub fn initial_commands(&self) -> Vec<Command> {
        if self.browse_mode {
            return vec![Command::LoadChats {
                offset: 0,
                limit: CHAT_PAGE_SIZE,
            }];
        }
        vec![
            Command::LoadChat {
                chat: self.from.id.clone(),
                before: None,
                limit: 50,
            },
            Command::MarkRead {
                chat: self.from.id.clone(),
            },
        ]
    }

    pub fn apply(&mut self, event: Event) {
        match event {
            Event::Link(link) => {
                if matches!(link, LinkStatus::Connected | LinkStatus::LoggedOut) {
                    self.pairing_qr = None;
                }
                self.link = link;
            }
            Event::PairingQr(qr) => {
                self.link = LinkStatus::Pairing;
                self.pairing_qr = Some(qr);
            }
            Event::Chats { offset, chats } => {
                self.chat_page_loading = false;
                let chats: Vec<_> = chats
                    .into_iter()
                    .filter(|chat| self.browse_mode || chat.id == self.from.id)
                    .collect();
                if offset == 0 {
                    self.chats = chats;
                } else {
                    for chat in chats {
                        if !self.chats.iter().any(|known| known.id == chat.id) {
                            self.chats.push(chat);
                        }
                    }
                }
                self.chats_exhausted = self.chats.len() < offset + CHAT_PAGE_SIZE;
            }
            Event::ChatUpdated(chat) => {
                if !self.browse_mode && chat.id != self.from.id {
                    return;
                }
                let selected_id = self
                    .chats
                    .get(self.selected_chat)
                    .map(|item| item.id.clone());
                if let Some(existing) = self.chats.iter_mut().find(|item| item.id == chat.id) {
                    *existing = chat;
                } else if !self.browse_mode || self.chats.len() < CHAT_PAGE_SIZE {
                    self.chats.push(chat);
                    if self.browse_mode {
                        self.chats_exhausted = false;
                    }
                }
                self.chats
                    .sort_by_key(|item| std::cmp::Reverse(item.last_activity));
                if let Some(selected_id) = selected_id {
                    self.selected_chat = self
                        .chats
                        .iter()
                        .position(|item| item.id == selected_id)
                        .unwrap_or(self.selected_chat);
                }
            }
            Event::ChatMessages {
                chat,
                before,
                messages,
            } => {
                if self.open_chat.as_deref() != Some(&chat) {
                    return;
                }
                if before.is_none() {
                    self.messages = messages;
                    self.older_loading = false;
                    return;
                }
                self.older_loading = false;
                if messages.is_empty() {
                    self.local_history_exhausted = true;
                    return;
                }
                self.local_history_exhausted = false;
                let known: std::collections::HashSet<_> = self
                    .messages
                    .iter()
                    .map(|message| message.id.as_str())
                    .collect();
                let mut older: Vec<_> = messages
                    .into_iter()
                    .filter(|message| !known.contains(message.id.as_str()))
                    .collect();
                older.append(&mut self.messages);
                self.messages = older;
            }
            Event::HistoryComplete { chat } => {
                if self.open_chat.as_deref() == Some(&chat) {
                    self.history_complete = true;
                    self.phone_history_requested = false;
                }
            }
            Event::SearchResults { query, messages } => {
                if query == self.search_query {
                    self.search_results = messages
                        .into_iter()
                        .filter(|m| m.chat == self.from.id)
                        .collect();
                }
            }
            Event::Message(message) => {
                if self.open_chat.as_deref() != Some(&message.chat) {
                    return;
                }
                self.phone_history_requested = false;
                if !self.messages.iter().any(|current| current.id == message.id) {
                    self.messages.push(message);
                    self.messages
                        .sort_by_key(|row| std::cmp::Reverse((row.timestamp, row.history_order)));
                }
            }
            Event::SendFinished {
                chat,
                text,
                quoting,
                result,
                ..
            } => {
                if result.is_ok() {
                    if self.to.id == chat && self.draft == text {
                        self.draft.clear();
                    }
                    if self.quoting == quoting {
                        self.quoting = None;
                    }
                    self.refusal = None;
                } else {
                    self.refusal = result.err();
                }
            }
        }
    }

    pub fn act(&mut self, action: InputAction) -> Vec<Command> {
        match action {
            InputAction::ScrollChats(delta) => {
                if self.open_chat.is_some() || self.chats.is_empty() {
                    return Vec::new();
                }
                let last = self.chats.len().saturating_sub(1);
                self.selected_chat = self.selected_chat.saturating_add_signed(delta).min(last);
                if self.selected_chat + 2 >= last && !self.chats_exhausted {
                    return self.act(InputAction::LoadMoreChats);
                }
                Vec::new()
            }
            InputAction::LoadMoreChats => {
                if !self.browse_mode || self.chats_exhausted || self.chat_page_loading {
                    return Vec::new();
                }
                let offset = self.chats.len();
                self.chat_page_loading = true;
                vec![Command::LoadChats {
                    offset,
                    limit: CHAT_PAGE_SIZE,
                }]
            }
            InputAction::SelectChat(index) => {
                if index < self.visible_chats().len() {
                    let id = self.visible_chats()[index].id.as_str().to_owned();
                    if let Some(actual) = self.chats.iter().position(|chat| chat.id == id) {
                        self.selected_chat = actual;
                    }
                }
                Vec::new()
            }
            InputAction::OpenSelected => self
                .selected_chat_id()
                .map(str::to_owned)
                .map(|chat| self.open(chat))
                .unwrap_or_default(),
            InputAction::OpenChat(chat) => self.open(chat),
            InputAction::Back => {
                if self.browse_mode && self.open_chat.is_some() {
                    self.open_chat = None;
                    self.to.id.clear();
                    self.to.name.clear();
                    self.from.id.clear();
                    self.from.name.clear();
                    self.messages.clear();
                    self.selected_message = None;
                    self.quoting = None;
                    self.search_query.clear();
                    self.search_results.clear();
                    self.refusal = None;
                    return Vec::new();
                }
                self.selected_message = None;
                self.quoting = None;
                self.search_query.clear();
                self.search_results.clear();
                self.refusal = None;
                Vec::new()
            }
            InputAction::BeginSearch => {
                self.search_active = true;
                self.compose_active = false;
                Vec::new()
            }
            InputAction::BeginCompose => {
                self.compose_active = true;
                self.search_active = false;
                Vec::new()
            }
            InputAction::EndSearch => {
                self.search_active = false;
                Vec::new()
            }
            InputAction::CancelInput => {
                self.search_active = false;
                self.compose_active = false;
                Vec::new()
            }
            InputAction::ToggleHelp => {
                self.show_help = !self.show_help;
                Vec::new()
            }
            InputAction::OlderPage => {
                if self.history_complete || self.older_loading {
                    return Vec::new();
                }
                let (Some(chat), Some(oldest)) = (self.open_chat.clone(), self.messages.last())
                else {
                    return Vec::new();
                };
                if self.local_history_exhausted {
                    if self.phone_history_requested {
                        return Vec::new();
                    }
                    self.phone_history_requested = true;
                    return self.request_older(oldest);
                }
                self.older_loading = true;
                vec![Command::LoadChat {
                    chat,
                    before: Some((oldest.timestamp, oldest.id.clone())),
                    limit: 50,
                }]
            }
            InputAction::Search(query) => {
                self.search_query = query.clone();
                self.search_results.clear();
                if query.trim().is_empty() {
                    if self.browse_mode && self.open_chat.is_none() {
                        self.selected_chat = 0;
                    }
                    Vec::new()
                } else if self.browse_mode && self.open_chat.is_none() {
                    self.selected_chat = self
                        .visible_chats()
                        .first()
                        .and_then(|chat| self.chats.iter().position(|item| item.id == chat.id))
                        .unwrap_or(self.selected_chat);
                    Vec::new()
                } else {
                    vec![Command::Search {
                        chat: self.from.id.clone(),
                        query,
                    }]
                }
            }
            InputAction::Compose(text) => {
                self.draft = text;
                self.refusal = None;
                self.compose_active = true;
                Vec::new()
            }
            InputAction::SelectMessage(index) => {
                self.selected_message = self
                    .active_messages()
                    .get(index)
                    .map(|message| message.id.clone());
                Vec::new()
            }
            InputAction::QuoteSelected => {
                self.refusal = None;
                if self.to.id != self.from.id {
                    self.quoting = None;
                    self.refusal = Some(
                        "Replies can quote messages only when --to and --from are the same chat."
                            .into(),
                    );
                } else if let Some(id) = self.selected_message.as_deref() {
                    self.quoting = Some(ReplyTarget::from(id));
                } else {
                    self.refusal = Some("Select a message with j/k before pressing r.".into());
                }
                Vec::new()
            }
            InputAction::Send => {
                self.compose_active = false;
                if self.draft.trim().is_empty() {
                    return Vec::new();
                }
                self.refusal = None;
                vec![Command::SendText {
                    chat: self.to.id.clone(),
                    text: self.draft.clone(),
                    quoting: self.quoting.clone(),
                }]
            }
            InputAction::Quit => {
                self.quit = true;
                vec![Command::Shutdown]
            }
        }
    }

    pub fn visible_chats(&self) -> Vec<&Chat> {
        let query = self.search_query.trim().to_lowercase();
        self.chats
            .iter()
            .filter(|chat| {
                query.is_empty()
                    || chat.name.to_lowercase().contains(&query)
                    || chat.id.to_lowercase().contains(&query)
            })
            .collect()
    }

    pub fn active_messages(&self) -> &[Message] {
        if self.search_query.is_empty() {
            &self.messages
        } else {
            &self.search_results
        }
    }

    fn selected_chat_id(&self) -> Option<&str> {
        self.chats
            .get(self.selected_chat)
            .map(|chat| chat.id.as_str())
    }

    fn open(&mut self, chat: String) -> Vec<Command> {
        if !self.browse_mode && chat != self.from.id {
            return Vec::new();
        }
        if self.browse_mode {
            let Some(selected) = self.chats.iter().find(|item| item.id == chat) else {
                return Vec::new();
            };
            self.to = ResolvedChat {
                id: selected.id.clone(),
                name: selected.name.clone(),
                is_group: selected.kind == crate::model::ChatKind::Group,
            };
            self.from = self.to.clone();
        }
        self.open_chat = Some(chat.clone());
        self.messages.clear();
        self.selected_message = None;
        self.quoting = None;
        self.refusal = None;
        self.history_complete = false;
        self.older_loading = false;
        self.phone_history_requested = false;
        self.local_history_exhausted = false;
        vec![
            Command::LoadChat {
                chat: chat.clone(),
                before: None,
                limit: 50,
            },
            Command::MarkRead { chat },
        ]
    }

    fn request_older(&self, oldest: &Message) -> Vec<Command> {
        self.open_chat
            .clone()
            .map(|chat| {
                vec![Command::FetchOlder {
                    chat,
                    id: oldest.id.clone(),
                    from_me: oldest.from_me,
                    timestamp: oldest.timestamp,
                }]
            })
            .unwrap_or_default()
    }
}

impl Default for SshState {
    fn default() -> Self {
        let placeholder = ResolvedChat {
            id: String::new(),
            name: String::new(),
            is_group: false,
        };
        Self::new(placeholder.clone(), placeholder)
    }
}

#[cfg(test)]
mod tests {
    use super::{InputAction, SshState};
    use crate::archive::Archive;
    use crate::model::{Content, Delivery, Message};
    use crate::ssh::backend::{Command, Event, LinkStatus};
    use crate::ssh::selector::ResolvedChat;

    fn direct(to: &str, from: &str) -> SshState {
        SshState::new(
            ResolvedChat {
                id: to.into(),
                name: format!("name-{to}"),
                is_group: to.ends_with("@g.us"),
            },
            ResolvedChat {
                id: from.into(),
                name: format!("name-{from}"),
                is_group: from.ends_with("@g.us"),
            },
        )
    }

    fn chat(id: &str, name: &str) -> crate::model::Chat {
        let archive = Archive::in_memory().unwrap();
        archive.ensure_chat(id, name).unwrap();
        archive.chats().unwrap().remove(0)
    }

    fn message(chat: &str, id: &str, timestamp: i64, from_me: bool) -> Message {
        Message {
            id: id.into(),
            chat: chat.into(),
            sender: chat.into(),
            sender_name: None,
            from_me,
            timestamp,
            history_order: None,
            content: Content::text(id),
            status: Delivery::None,
            delivered_at: None,
            read_at: None,
            quoted: None,
            reactions: Vec::new(),
            edited: false,
            mentions: Vec::new(),
            forwarded: false,
            thumbnail: None,
        }
    }

    #[test]
    fn open_chat_loads_and_marks_read() {
        let id = "1@s.whatsapp.net";
        let state = direct("2@s.whatsapp.net", id);
        let commands = state.initial_commands();
        assert!(commands.iter().any(|command| matches!(command,
            Command::LoadChat { chat, before: None, .. } if chat == id)));
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, Command::MarkRead { chat } if chat == id))
        );
    }

    #[test]
    fn older_pages_prepend_once_and_stop_at_complete() {
        let id = "1@s.whatsapp.net";
        let mut state = direct(id, id);
        state.messages = vec![message(id, "new", 200, false)];
        let first = state.act(InputAction::OlderPage);
        assert!(
            matches!(first.as_slice(), [Command::LoadChat { before: Some((200, key)), .. }] if key == "new")
        );
        let older = vec![message(id, "old", 100, false)];
        state.apply(Event::ChatMessages {
            chat: id.into(),
            before: Some((200, "new".into())),
            messages: older,
        });
        assert_eq!(
            state
                .messages
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            ["old", "new"]
        );
        state.apply(Event::HistoryComplete { chat: id.into() });
        assert!(state.act(InputAction::OlderPage).is_empty());
    }

    #[test]
    fn search_filters_titles_and_ignores_stale_results() {
        let mut state = direct("1@s.whatsapp.net", "1@s.whatsapp.net");
        state.chats = vec![
            chat("1@s.whatsapp.net", "Ada"),
            chat("2@s.whatsapp.net", "Bob"),
        ];
        assert!(matches!(
            state.act(InputAction::Search("ada".into())).as_slice(),
            [Command::Search { chat, query }] if chat == "1@s.whatsapp.net" && query == "ada"
        ));
        assert_eq!(
            state
                .visible_chats()
                .iter()
                .map(|chat| chat.name.as_str())
                .collect::<Vec<_>>(),
            ["Ada"]
        );
        state.apply(Event::SearchResults {
            query: "old query".into(),
            messages: vec![message("x", "stale", 1, false)],
        });
        assert!(state.search_results.is_empty());
    }

    #[test]
    fn quoted_send_uses_selected_reply_target() {
        let id = "1@s.whatsapp.net";
        let mut state = direct("1@s.whatsapp.net", "1@s.whatsapp.net");
        state.open_chat = Some(id.into());
        state.messages = vec![message(id, "source", 1, false)];
        state.act(InputAction::SelectMessage(0));
        state.act(InputAction::QuoteSelected);
        state.act(InputAction::Compose("reply".into()));
        let commands = state.act(InputAction::Send);
        assert!(
            matches!(commands.as_slice(), [Command::SendText { chat, text, quoting: Some(target) }]
            if chat == id && text == "reply" && target.id == "source")
        );
    }

    #[test]
    fn refused_send_keeps_draft_and_reason() {
        let id = "1@s.whatsapp.net";
        let mut state = direct(id, id);
        state.act(InputAction::Compose("keep me".into()));
        state.apply(Event::SendFinished {
            id: String::new(),
            chat: id.into(),
            text: "keep me".into(),
            quoting: None,
            result: Err("offline".into()),
        });
        assert_eq!(state.draft, "keep me");
        assert_eq!(state.refusal.as_deref(), Some("offline"));
    }

    #[test]
    fn incoming_message_only_changes_the_open_chat() {
        let open = "1@s.whatsapp.net";
        let mut state = direct(open, open);
        state.apply(Event::Message(message(
            "2@s.whatsapp.net",
            "other",
            2,
            false,
        )));
        assert!(state.messages.is_empty());
        state.apply(Event::Message(message(open, "current", 3, false)));
        assert_eq!(
            state
                .messages
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            ["current"]
        );
    }

    #[test]
    fn link_status_is_visible_to_the_terminal() {
        let mut state = SshState::default();
        state.apply(Event::Link(LinkStatus::Connected));
        assert!(matches!(state.link, LinkStatus::Connected));
    }

    #[test]
    fn group_transcript_accepts_each_member_but_no_other_chat() {
        let group = "team@g.us";
        let mut state = direct("target@s.whatsapp.net", group);
        state.apply(Event::Message(message(group, "from-a", 2, false)));
        let mut member = message(group, "from-b", 3, false);
        member.sender = "member-2@s.whatsapp.net".into();
        state.apply(Event::Message(member));
        state.apply(Event::Message(message("other@g.us", "outside", 4, false)));
        assert_eq!(
            state
                .messages
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>(),
            ["from-b", "from-a"]
        );
        assert!(
            matches!(state.act(InputAction::OlderPage).as_slice(), [Command::LoadChat { chat, .. }] if chat == group)
        );
        state.act(InputAction::Compose("group reply".into()));
        assert!(
            matches!(state.act(InputAction::Send).as_slice(), [Command::SendText { chat, .. }] if chat == "target@s.whatsapp.net")
        );
    }

    #[test]
    fn initial_load_and_read_are_only_for_from() {
        let state = direct("to@s.whatsapp.net", "from@g.us");
        let commands = state.initial_commands();
        assert!(
            matches!(commands.as_slice(), [Command::LoadChat { chat, before: None, .. }, Command::MarkRead { chat: marked }] if chat == "from@g.us" && marked == chat)
        );
        assert!(
            !commands
                .iter()
                .any(|command| matches!(command, Command::FetchOlder { .. }))
        );
    }

    #[test]
    fn phone_history_is_requested_only_after_explicit_older_action() {
        let id = "from@g.us";
        let mut state = direct("to@s.whatsapp.net", id);
        state.messages = vec![message(id, "oldest", 1, false)];
        state.apply(Event::ChatMessages {
            chat: id.into(),
            before: Some((1, "oldest".into())),
            messages: vec![],
        });
        let commands = state.act(InputAction::OlderPage);
        assert!(
            matches!(commands.as_slice(), [Command::FetchOlder { chat, id: message, .. }] if chat == id && message == "oldest")
        );
    }

    #[test]
    fn back_keeps_the_selected_conversation_open() {
        let mut state = direct("to@s.whatsapp.net", "from@g.us");
        state.act(InputAction::Back);
        assert_eq!(state.open_chat.as_deref(), Some("from@g.us"));
    }
}
