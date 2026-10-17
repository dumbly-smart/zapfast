//! Terminal-facing reducer: views render this state and turn keys into actions.

use crate::model::{Chat, Message, ReplyTarget};

use super::backend::{Command, Event, LinkStatus};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputAction {
    SelectChat(usize),
    OpenSelected,
    OpenChat(String),
    Back,
    OlderPage,
    Search(String),
    Compose(String),
    SelectMessage(usize),
    QuoteSelected,
    Send,
    Quit,
}

#[derive(Debug)]
pub struct SshState {
    pub link: LinkStatus,
    pub pairing_qr: Option<String>,
    pub chats: Vec<Chat>,
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
    older_loading: bool,
    phone_history_requested: bool,
    local_history_exhausted: bool,
}

impl SshState {
    pub fn new() -> Self {
        Self {
            link: LinkStatus::Starting,
            pairing_qr: None,
            chats: Vec::new(),
            selected_chat: 0,
            open_chat: None,
            messages: Vec::new(),
            selected_message: None,
            draft: String::new(),
            quoting: None,
            refusal: None,
            search_query: String::new(),
            search_results: Vec::new(),
            history_complete: false,
            quit: false,
            older_loading: false,
            phone_history_requested: false,
            local_history_exhausted: false,
        }
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
            Event::Chats(chats) => {
                let selected = self.selected_chat_id().map(str::to_owned);
                self.chats = chats;
                self.selected_chat = selected
                    .as_deref()
                    .and_then(|id| self.chats.iter().position(|chat| chat.id == id))
                    .unwrap_or_else(|| self.selected_chat.min(self.chats.len().saturating_sub(1)));
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
                    self.search_results = messages;
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
                    if self.open_chat.as_deref() == Some(&chat) && self.draft == text {
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
                self.open_chat = None;
                self.messages.clear();
                self.selected_message = None;
                self.quoting = None;
                self.history_complete = false;
                self.older_loading = false;
                self.phone_history_requested = false;
                self.local_history_exhausted = false;
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
                    Vec::new()
                } else {
                    vec![Command::Search { query }]
                }
            }
            InputAction::Compose(text) => {
                self.draft = text;
                self.refusal = None;
                Vec::new()
            }
            InputAction::SelectMessage(index) => {
                self.selected_message = self.messages.get(index).map(|message| message.id.clone());
                Vec::new()
            }
            InputAction::QuoteSelected => {
                self.quoting = self.selected_message.as_deref().map(ReplyTarget::from);
                Vec::new()
            }
            InputAction::Send => {
                let (Some(chat), false) = (self.open_chat.clone(), self.draft.trim().is_empty())
                else {
                    return Vec::new();
                };
                self.refusal = None;
                vec![Command::SendText {
                    chat,
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

    fn selected_chat_id(&self) -> Option<&str> {
        self.chats
            .get(self.selected_chat)
            .map(|chat| chat.id.as_str())
    }

    fn open(&mut self, chat: String) -> Vec<Command> {
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
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{InputAction, SshState};
    use crate::archive::Archive;
    use crate::model::{Content, Delivery, Message};
    use crate::ssh::backend::{Command, Event, LinkStatus};

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
        let mut state = SshState::new();
        state.chats = vec![chat(id, "Ada")];
        let commands = state.act(InputAction::OpenChat(id.into()));
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
        let mut state = SshState::new();
        state.open_chat = Some(id.into());
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
        let mut state = SshState::new();
        state.chats = vec![
            chat("1@s.whatsapp.net", "Ada"),
            chat("2@s.whatsapp.net", "Bob"),
        ];
        assert!(matches!(
            state.act(InputAction::Search("ada".into())).as_slice(),
            [Command::Search { query }] if query == "ada"
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
        let mut state = SshState::new();
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
        let mut state = SshState::new();
        state.open_chat = Some(id.into());
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
        let mut state = SshState::new();
        state.open_chat = Some(open.into());
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
        let mut state = SshState::new();
        state.apply(Event::Link(LinkStatus::Connected));
        assert!(matches!(state.link, LinkStatus::Connected));
    }
}
