//! Headless WhatsApp session and encrypted archive adapter.
//!
//! Protocol operations stay in the pinned whatsapp-rust library. This module
//! translates its message events to ZapFast's shared archive model and exposes
//! only the text operations needed by the SSH client.

use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};

use anyhow::{Context, Result};
use tokio::sync::mpsc as tokio_mpsc;
use whatsapp_rust::pair::NATIVE_CAMERA_DEEP_LINK_PREFIX;
use whatsapp_rust::prelude::{Bot, Jid, MessageBuilderExt, MessageExt, wa};
use whatsapp_rust::store::SqliteStore;
use whatsapp_rust::types::events::{Event as WaEvent, EventHandler};
use whatsapp_rust::waproto::buffa::Message as _;
use whatsapp_rust::{AppStateResyncMode, WAPatchName};
use zeroize::Zeroizing;

use crate::archive::Archive;
use crate::model::{Chat, Content, Delivery, Message, Quoted, ReplyTarget};
use crate::paths::AccountDirs;

#[derive(Clone)]
pub struct BackendWake(Arc<dyn Fn() + Send + Sync>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectFilter {
    pub to: String,
    pub from: String,
}

impl BackendWake {
    pub fn new(f: impl Fn() + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }
    pub fn wake(&self) {
        (self.0)();
    }
}

pub enum ArchiveUnlock {
    PlatformKeyring,
    Supplied(Zeroizing<[u8; 32]>),
}

#[derive(Clone, Debug)]
pub enum LinkStatus {
    Starting,
    Pairing,
    Connected,
    Disconnected,
    LoggedOut,
    Error(String),
}

pub enum Event {
    Link(LinkStatus),
    PairingQr(String),
    Chats {
        archived: bool,
        offset: usize,
        chats: Vec<Chat>,
    },
    ChatUpdated(Chat),
    ChatMessages {
        chat: String,
        before: Option<(i64, String)>,
        messages: Vec<Message>,
    },
    HistoryComplete {
        chat: String,
    },
    SearchResults {
        query: String,
        messages: Vec<Message>,
    },
    Message(Message),
    SendFinished {
        id: String,
        chat: String,
        text: String,
        quoting: Option<ReplyTarget>,
        result: std::result::Result<(), String>,
    },
}

pub enum Command {
    LoadChats {
        archived: bool,
        offset: usize,
        limit: usize,
    },
    LoadChat {
        chat: String,
        before: Option<(i64, String)>,
        limit: usize,
    },
    FetchOlder {
        chat: String,
        id: String,
        from_me: bool,
        timestamp: i64,
    },
    Search {
        chat: String,
        query: String,
    },
    SendText {
        chat: String,
        text: String,
        quoting: Option<ReplyTarget>,
    },
    MarkRead {
        chat: String,
    },
    Shutdown,
}

pub struct Backend {
    commands: tokio_mpsc::UnboundedSender<Command>,
    events: mpsc::Receiver<Event>,
    thread: Option<JoinHandle<()>>,
}

impl Backend {
    pub fn spawn_headless(
        dirs: AccountDirs,
        wake: BackendWake,
        unlock: ArchiveUnlock,
        filter: DirectFilter,
    ) -> Self {
        let (command_tx, command_rx) = tokio_mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("zapfast-ssh-backend".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        emit(
                            &event_tx,
                            &wake,
                            Event::Link(LinkStatus::Error(error.to_string())),
                        );
                        return;
                    }
                };
                runtime.block_on(run(dirs, command_rx, event_tx, wake, unlock, filter));
            })
            .expect("could not start ZapFast SSH backend thread");
        Self {
            commands: command_tx,
            events: event_rx,
            thread: Some(thread),
        }
    }

    pub fn send(&self, command: Command) -> Result<()> {
        self.commands
            .send(command)
            .context("WhatsApp backend has stopped")
    }

    pub fn try_recv(&self) -> Option<Event> {
        self.events.try_recv().ok()
    }

    pub fn shutdown(mut self) {
        let _ = self.commands.send(Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct ForwardEvents(async_channel::Sender<Arc<WaEvent>>);

impl EventHandler for ForwardEvents {
    fn handle_event(&self, event: Arc<WaEvent>) {
        // This handler runs inline on the protocol dispatch path. Never block
        // it on terminal I/O or archive work.
        let _ = self.0.try_send(event);
    }
}

async fn run(
    dirs: AccountDirs,
    mut commands: tokio_mpsc::UnboundedReceiver<Command>,
    events: mpsc::Sender<Event>,
    wake: BackendWake,
    unlock: ArchiveUnlock,
    filter: DirectFilter,
) {
    emit(&events, &wake, Event::Link(LinkStatus::Starting));
    if let Err(error) = dirs.ensure() {
        emit(
            &events,
            &wake,
            Event::Link(LinkStatus::Error(error.to_string())),
        );
        return;
    }
    let key = match unlock {
        ArchiveUnlock::Supplied(key) => key,
        ArchiveUnlock::PlatformKeyring => {
            emit(
                &events,
                &wake,
                Event::Link(LinkStatus::Error(
                    "The SSH client requires its passphrase vault".into(),
                )),
            );
            return;
        }
    };
    let archive = match Archive::open_with_key(&dirs.archive_db(), &key) {
        Ok(archive) => archive,
        Err(error) => {
            emit(
                &events,
                &wake,
                Event::Link(LinkStatus::Error(format!(
                    "Could not unlock archive: {error:#}"
                ))),
            );
            return;
        }
    };
    drop(key);
    let store = match SqliteStore::open(&dirs.session_db().to_string_lossy()).await {
        Ok(store) => store,
        Err(error) => {
            emit(
                &events,
                &wake,
                Event::Link(LinkStatus::Error(error.to_string())),
            );
            return;
        }
    };
    // A full app-state replay may dispatch more than 256 mutations at once.
    // Keep every event so archived chats cannot be classified from a partial replay.
    let (handler, wa_events) = async_channel::unbounded::<Arc<WaEvent>>();
    let bot = match Bot::builder()
        .with_backend(store)
        .with_event_handler(ForwardEvents(handler))
        .build()
        .await
    {
        Ok(bot) => bot,
        Err(error) => {
            emit(
                &events,
                &wake,
                Event::Link(LinkStatus::Error(error.to_string())),
            );
            return;
        }
    };
    let handle = bot.spawn();
    let client = handle.client();
    let archive_repair_needed = filter.from.is_empty()
        && archive
            .meta("ssh_archive_state_v1")
            .ok()
            .flatten()
            .is_none();
    let mut archive_repair_pending = archive_repair_needed;
    let mut archive_repair_started = false;
    let mut deferred_chat_page = None;
    let (repair_tx, mut repair_rx) = tokio_mpsc::unbounded_channel();
    let mut stop = false;
    while !stop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(Command::Shutdown) | None => stop = true,
                Some(command @ Command::LoadChats { .. }) if archive_repair_pending => {
                    deferred_chat_page = Some(command);
                }
                Some(command) => process_command(command, &filter, &archive, &client, &events, &wake).await,
            },
            event = wa_events.recv() => match event {
                Ok(event) => {
                    if matches!(&*event, WaEvent::Connected(_))
                        && archive_repair_pending
                        && !archive_repair_started
                    {
                        archive_repair_started = true;
                        let client = client.clone();
                        let repair_tx = repair_tx.clone();
                        tokio::spawn(async move {
                            let result = client
                                .resync_app_state(
                                    [WAPatchName::RegularLow],
                                    AppStateResyncMode::Snapshot,
                                )
                                .await;
                            let _ = repair_tx.send(result);
                        });
                    }
                    process_wa_event(&event, &filter, &archive, &events, &wake);
                }
                Err(_) => stop = true,
            },
            repair = repair_rx.recv(), if archive_repair_pending && archive_repair_started => {
                // The protocol dispatches snapshot mutations before returning.
                // Apply the queued events before opening the first chat page.
                while let Ok(event) = wa_events.try_recv() {
                    process_wa_event(&event, &filter, &archive, &events, &wake);
                }
                match repair {
                    Some(Ok(report)) if report.all_synced() => {
                        if let Err(error) = archive.set_meta("ssh_archive_state_v1", "done") {
                            emit(&events, &wake, Event::Link(LinkStatus::Error(error.to_string())));
                        }
                        archive_repair_pending = false;
                        if let Some(command) = deferred_chat_page.take() {
                            process_command(command, &filter, &archive, &client, &events, &wake).await;
                        }
                    }
                    Some(Ok(report)) => {
                        archive_repair_started = false;
                        emit(&events, &wake, Event::Link(LinkStatus::Error(format!(
                            "Could not sync archived chats: {:?}",
                            report.unsynced().collect::<Vec<_>>()
                        ))));
                    }
                    Some(Err(error)) => {
                        archive_repair_started = false;
                        emit(&events, &wake, Event::Link(LinkStatus::Error(format!(
                            "Could not sync archived chats: {error}"
                        ))));
                    }
                    None => stop = true,
                }
            },
        }
    }
    drop(client);
    let _ = handle.shutdown().await;
}

async fn process_command(
    command: Command,
    filter: &DirectFilter,
    archive: &Archive,
    client: &Arc<whatsapp_rust::Client>,
    events: &mpsc::Sender<Event>,
    wake: &BackendWake,
) {
    match command {
        Command::LoadChats {
            archived,
            offset,
            limit,
        } => match archive.chats_page(archived, offset, limit) {
            Ok(chats) => emit(
                events,
                wake,
                Event::Chats {
                    archived,
                    offset,
                    chats,
                },
            ),
            Err(error) => emit(
                events,
                wake,
                Event::Link(LinkStatus::Error(error.to_string())),
            ),
        },
        Command::LoadChat {
            chat,
            before,
            limit,
        } => match archive.messages(
            &chat,
            before
                .as_ref()
                .map(|(timestamp, id)| (*timestamp, id.as_str())),
            limit,
        ) {
            Ok(messages) if filter.from.is_empty() || chat == filter.from => emit(
                events,
                wake,
                Event::ChatMessages {
                    chat,
                    before,
                    messages,
                },
            ),
            Ok(_) => {}
            Err(error) => emit(
                events,
                wake,
                Event::Link(LinkStatus::Error(error.to_string())),
            ),
        },
        Command::FetchOlder {
            chat,
            id,
            from_me,
            timestamp,
        } if filter.from.is_empty() || chat == filter.from => match chat.parse::<Jid>() {
            Ok(jid) => {
                if let Err(error) = client
                    .fetch_message_history(&jid, &id, from_me, timestamp, 50)
                    .await
                {
                    emit(
                        events,
                        wake,
                        Event::Link(LinkStatus::Error(format!(
                            "Could not request older messages: {error}"
                        ))),
                    );
                }
            }
            Err(error) => emit(
                events,
                wake,
                Event::Link(LinkStatus::Error(error.to_string())),
            ),
        },
        Command::Search { chat, query } if filter.from.is_empty() || chat == filter.from => {
            match archive.search_chat_messages(&chat, &query, None, None, 100) {
                Ok(messages) => emit(events, wake, Event::SearchResults { query, messages }),
                Err(error) => emit(
                    events,
                    wake,
                    Event::Link(LinkStatus::Error(error.to_string())),
                ),
            }
        }
        Command::MarkRead { chat } if filter.from.is_empty() || chat == filter.from => {
            if let Err(error) = archive.mark_read(&chat) {
                emit(
                    events,
                    wake,
                    Event::Link(LinkStatus::Error(error.to_string())),
                );
            }
        }
        Command::SendText {
            chat,
            text,
            quoting,
        } if filter.to.is_empty() || chat == filter.to => {
            if text.trim().is_empty() {
                return;
            }
            let jid: Jid = match chat.parse() {
                Ok(jid) => jid,
                Err(error) => {
                    emit(
                        events,
                        wake,
                        Event::Link(LinkStatus::Error(error.to_string())),
                    );
                    return;
                }
            };
            let quote = match quoting.as_ref() {
                Some(target) => match quote_context(archive, &chat, target) {
                    Ok(quote) => Some(quote),
                    Err(error) => {
                        emit(
                            events,
                            wake,
                            Event::SendFinished {
                                id: String::new(),
                                chat,
                                text,
                                quoting,
                                result: Err(error),
                            },
                        );
                        return;
                    }
                },
                None => None,
            };
            let id = client.generate_message_id().into_string();
            let timestamp = jiff::Timestamp::now().as_second();
            let row = Message {
                id: id.clone(),
                chat: chat.clone().into(),
                sender: String::new(),
                sender_name: None,
                from_me: true,
                timestamp,
                history_order: None,
                content: Content::text(text.clone()),
                status: Delivery::Pending,
                delivered_at: None,
                read_at: None,
                quoted: quote.as_ref().map(|(_, shown)| shown.clone()),
                reactions: Vec::new(),
                edited: false,
                mentions: Vec::new(),
                forwarded: false,
                thumbnail: None,
            };
            let body = match quote {
                Some((context, _)) => wa::Message::text_with_context(text.clone(), context),
                None => wa::Message::text(text.clone()),
            };
            let raw = body.encode_to_vec();
            if let Err(error) = archive
                .ensure_chat(&chat, &chat)
                .and_then(|_| archive.insert_message(&row, Some(&raw)))
            {
                emit(
                    events,
                    wake,
                    Event::SendFinished {
                        id,
                        chat,
                        text,
                        quoting,
                        result: Err(error.to_string()),
                    },
                );
                return;
            }
            let result = client.send_message(jid, body).await;
            let delivered = match result {
                Ok(_) => archive
                    .set_status(&chat, &id, Delivery::Sent, timestamp)
                    .map(|_| ())
                    .map_err(|error| error.to_string()),
                Err(error) => {
                    let _ = archive.set_status(&chat, &id, Delivery::Failed, timestamp);
                    Err(error.to_string())
                }
            };
            emit(
                events,
                wake,
                Event::SendFinished {
                    id,
                    chat,
                    text,
                    quoting,
                    result: delivered,
                },
            );
            emit_selected_chat(archive, &filter.from, events, wake);
        }
        Command::Shutdown => {}
        _ => {}
    }
}

fn process_wa_event(
    event: &WaEvent,
    filter: &DirectFilter,
    archive: &Archive,
    events: &mpsc::Sender<Event>,
    wake: &BackendWake,
) {
    use whatsapp_rust::types::events::Event as WaEvent;
    match event {
        WaEvent::PairingQrCode(qr) => {
            emit(events, wake, Event::Link(LinkStatus::Pairing));
            emit(
                events,
                wake,
                Event::PairingQr(format!("{NATIVE_CAMERA_DEEP_LINK_PREFIX}{}", qr.code)),
            );
        }
        WaEvent::Connected(_) => emit(events, wake, Event::Link(LinkStatus::Connected)),
        WaEvent::Disconnected(_) => emit(events, wake, Event::Link(LinkStatus::Disconnected)),
        WaEvent::LoggedOut(_) => emit(events, wake, Event::Link(LinkStatus::LoggedOut)),
        WaEvent::ArchiveUpdate(update) => {
            let chat = update.jid.to_string();
            let stored = archive.ensure_chat(&chat, &chat).and_then(|_| {
                archive.set_archived_at(
                    &chat,
                    update.action.archived.unwrap_or(false),
                    update.timestamp.timestamp_millis(),
                )
            });
            match stored {
                Ok(()) => emit_selected_chat(archive, &chat, events, wake),
                Err(error) => emit(
                    events,
                    wake,
                    Event::Link(LinkStatus::Error(error.to_string())),
                ),
            }
        }
        WaEvent::HistorySync(sync) => {
            if let Some(sync) = sync.get() {
                for conversation in &sync.conversations {
                    if filter.from.is_empty() || conversation.id == filter.from {
                        store_history_conversation(conversation, filter, archive, events, wake);
                    }
                }
                if sync.sync_type == wa::history_sync::HistorySyncType::ON_DEMAND
                    && sync.progress == Some(100)
                {
                    for conversation in &sync.conversations {
                        if !filter.from.is_empty() && conversation.id != filter.from {
                            continue;
                        }
                        emit(
                            events,
                            wake,
                            Event::HistoryComplete {
                                chat: conversation.id.clone(),
                            },
                        );
                    }
                }
                emit_selected_chat(archive, &filter.from, events, wake);
            }
        }
        WaEvent::Messages(batch) => {
            for inbound in batch.iter() {
                let chat = inbound.info.source.chat.to_string();
                if !accept_live_chat(filter, &chat, inbound.info.source.is_from_me) {
                    continue;
                }
                let sender = inbound.info.source.sender.to_string();
                let content = inbound
                    .message
                    .text_content()
                    .map(Content::text)
                    .unwrap_or_else(|| Content::Unsupported {
                        what: "Non-text message".into(),
                    });
                let row = Message {
                    id: inbound.info.id.to_string(),
                    chat: chat.clone().into(),
                    sender,
                    sender_name: (!inbound.info.push_name.is_empty())
                        .then(|| inbound.info.push_name.to_string()),
                    from_me: inbound.info.source.is_from_me,
                    timestamp: inbound.info.timestamp.timestamp(),
                    history_order: None,
                    content,
                    status: if inbound.info.source.is_from_me {
                        Delivery::Sent
                    } else {
                        Delivery::None
                    },
                    delivered_at: None,
                    read_at: None,
                    quoted: None,
                    reactions: Vec::new(),
                    edited: false,
                    mentions: Vec::new(),
                    forwarded: false,
                    thumbnail: None,
                };
                let raw = inbound.message.encode_to_vec();
                let name = row.sender_name.as_deref().unwrap_or(&chat);
                let stored = archive
                    .ensure_chat(&chat, name)
                    .and_then(|_| archive.touch_activity(&chat, row.timestamp))
                    .and_then(|_| archive.insert_message(&row, Some(&raw)));
                if let Err(error) = stored {
                    emit(
                        events,
                        wake,
                        Event::Link(LinkStatus::Error(error.to_string())),
                    );
                    continue;
                }
                emit_selected_chat(archive, &chat, events, wake);
                if !row.from_me {
                    let _ = archive.bump_unread(&chat);
                }
                if filter.from.is_empty() || row.chat == filter.from {
                    emit(events, wake, Event::Message(row));
                }
            }
            emit_selected_chat(archive, &filter.from, events, wake);
        }
        _ => {}
    }
}

fn store_history_conversation(
    conversation: &whatsapp_rust::prelude::wa::Conversation,
    filter: &DirectFilter,
    archive: &Archive,
    events: &mpsc::Sender<Event>,
    wake: &BackendWake,
) {
    let chat = conversation.id.as_str();
    if !filter.from.is_empty() && chat != filter.from {
        return;
    }
    let name = conversation
        .name
        .as_deref()
        .filter(|name| !name.is_empty())
        .unwrap_or(chat);
    if let Err(error) = archive.ensure_chat(chat, name) {
        emit(
            events,
            wake,
            Event::Link(LinkStatus::Error(error.to_string())),
        );
        return;
    }
    if let Some(archived) = conversation.archived {
        if let Err(error) = archive.set_archived_snapshot(chat, archived) {
            emit(
                events,
                wake,
                Event::Link(LinkStatus::Error(error.to_string())),
            );
            return;
        }
    }
    if let Some(unread) = conversation.unread_count {
        let _ = archive.history_unread(chat, unread);
    }
    for entry in &conversation.messages {
        let Some(info) = entry.message.as_option() else {
            continue;
        };
        let Some(key) = info.key.as_option() else {
            continue;
        };
        let Some(id) = key.id.as_deref().filter(|id| !id.is_empty()) else {
            continue;
        };
        let timestamp = info.message_timestamp.unwrap_or(0) as i64;
        let from_me = key.from_me.unwrap_or(false);
        let sender = key
            .participant
            .as_deref()
            .or(info.participant.as_deref())
            .or(key.remote_jid.as_deref())
            .unwrap_or(chat)
            .to_owned();
        let content = info
            .message
            .as_option()
            .and_then(MessageExt::text_content)
            .map(Content::text)
            .unwrap_or_else(|| Content::Unsupported {
                what: "Non-text message".into(),
            });
        let row = Message {
            id: id.to_owned(),
            chat: chat.to_owned().into(),
            sender,
            sender_name: info.push_name.clone().filter(|name| !name.is_empty()),
            from_me,
            timestamp,
            history_order: entry
                .msg_order_id
                .and_then(|order| i64::try_from(order).ok()),
            content,
            status: if from_me {
                Delivery::Sent
            } else {
                Delivery::None
            },
            delivered_at: None,
            read_at: None,
            quoted: None,
            reactions: Vec::new(),
            edited: false,
            mentions: Vec::new(),
            forwarded: false,
            thumbnail: None,
        };
        let raw = info.encode_to_vec();
        if let Err(error) = archive.insert_message(&row, Some(&raw)) {
            emit(
                events,
                wake,
                Event::Link(LinkStatus::Error(error.to_string())),
            );
            continue;
        }
        let _ = archive.touch_activity(chat, timestamp);
        emit(events, wake, Event::Message(row));
    }
    emit_selected_chat(archive, chat, events, wake);
}

fn emit_selected_chat(
    archive: &Archive,
    id: &str,
    events: &mpsc::Sender<Event>,
    wake: &BackendWake,
) {
    if let Ok(Some(chat)) = archive.chat(id) {
        emit(events, wake, Event::ChatUpdated(chat));
    }
}

fn accept_live_chat(filter: &DirectFilter, chat: &str, from_me: bool) -> bool {
    if from_me {
        filter.to.is_empty() || chat == filter.to
    } else {
        filter.from.is_empty() || chat == filter.from
    }
}

fn quote_context(
    archive: &Archive,
    destination: &str,
    target: &ReplyTarget,
) -> std::result::Result<(wa::ContextInfo, Quoted), String> {
    let source = target.source_chat(destination);
    if target.id.is_empty() {
        return Err("The quoted message is unavailable".into());
    }
    let row = archive
        .message(source, &target.id)
        .map_err(|_| "The quoted message is unavailable".to_owned())?
        .ok_or_else(|| "The quoted message is unavailable".to_owned())?;
    if matches!(row.content, Content::Revoked) {
        return Err("The quoted message is unavailable".into());
    }
    if source != destination
        && (row.private_reply_recipient().is_none() || row.sender != destination)
    {
        return Err("That cross-chat reply is unavailable".into());
    }
    let raw = archive
        .raw(source, &target.id)
        .map_err(|_| "The quoted message is unavailable".to_owned())?
        .ok_or_else(|| "The quoted message is unavailable".to_owned())?;
    let original = wa::Message::decode_from_slice(&raw)
        .map_err(|_| "The quoted message is unavailable".to_owned())?;
    let destination_jid: Jid = destination
        .parse()
        .map_err(|_| "The destination chat is invalid".to_owned())?;
    let source_jid: Jid = source
        .parse()
        .map_err(|_| "The quoted chat is invalid".to_owned())?;
    let sender_text = if row.from_me {
        destination
    } else {
        row.sender.as_str()
    };
    let sender: Jid = sender_text
        .parse()
        .map_err(|_| "The quoted sender is unavailable".to_owned())?;
    let mut context = whatsapp_rust::wacore::proto_helpers::build_quote_context_with_info(
        row.id.clone(),
        &sender,
        &source_jid,
        &destination_jid,
        &original,
    );
    // A quoted @all stays visible but must not notify the group again.
    let bits = context.non_jid_mentions.unwrap_or(0) & !1;
    context.non_jid_mentions = (bits != 0).then_some(bits);
    let shown = Quoted {
        chat: (source != destination).then(|| source.to_owned()),
        id: row.id,
        sender: sender_text.to_owned(),
        sender_name: if row.from_me {
            Some("You".into())
        } else {
            row.sender_name.clone()
        },
        summary: row.content.summary(),
        mentions: row.mentions,
    };
    Ok((context, shown))
}

fn emit(events: &mpsc::Sender<Event>, wake: &BackendWake, event: Event) {
    if events.send(event).is_ok() {
        wake.wake();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BackendWake, DirectFilter, Event, accept_live_chat, emit_selected_chat, process_wa_event,
        store_history_conversation,
    };
    use crate::archive::Archive;
    use crate::model::Content;
    use whatsapp_rust::prelude::{MessageField, wa};
    use whatsapp_rust::types::events::{Event as WaEvent, PairingQrCode};

    #[test]
    fn pairing_qr_contains_the_linked_devices_camera_url() {
        let qr = PairingQrCode::builder()
            .code("fixture-pairing-token".into())
            .timeout(std::time::Duration::from_secs(20))
            .build();
        let archive = Archive::in_memory().unwrap();
        let (events, received) = std::sync::mpsc::channel();
        process_wa_event(
            &WaEvent::PairingQrCode(qr),
            &DirectFilter {
                to: "to".into(),
                from: "from".into(),
            },
            &archive,
            &events,
            &BackendWake::new(|| {}),
        );
        let _status = received.recv().unwrap();
        assert!(matches!(
            received.recv().unwrap(),
            Event::PairingQr(payload) if payload == "https://wa.me/settings/linked_devices#fixture-pairing-token"
        ));
    }

    #[test]
    fn wake_notifies_receiver() {
        let (tx, rx) = std::sync::mpsc::channel();
        let wake = BackendWake::new(move || tx.send(()).unwrap());
        wake.wake();
        rx.try_recv().unwrap();
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn history_sync_files_text_to_the_shared_archive() {
        let chat = "1@s.whatsapp.net";
        let mut conversation = wa::Conversation::default();
        conversation.id = chat.into();
        conversation.name = Some("Fixture contact".into());
        conversation.unread_count = Some(1);
        conversation.messages.push({
            let mut entry = wa::HistorySyncMsg::default();
            entry.msg_order_id = Some(8);
            entry.message = MessageField::some({
                let mut info = wa::WebMessageInfo::default();
                info.key = MessageField::some({
                    let mut key = wa::MessageKey::default();
                    key.remote_jid = Some(chat.into());
                    key.from_me = Some(false);
                    key.id = Some("history-message".into());
                    key
                });
                info.message_timestamp = Some(100);
                info.push_name = Some("Fixture contact".into());
                info.message = MessageField::some({
                    let mut message = wa::Message::default();
                    message.conversation = Some("hello from history".into());
                    message
                });
                info
            });
            entry
        });

        let archive = Archive::in_memory().unwrap();
        let (events, _receiver) = std::sync::mpsc::channel();
        let filter = DirectFilter {
            to: chat.into(),
            from: chat.into(),
        };
        store_history_conversation(
            &conversation,
            &filter,
            &archive,
            &events,
            &BackendWake::new(|| {}),
        );
        let rows = archive.messages(chat, None, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "history-message");
        assert_eq!(rows[0].history_order, Some(8));
        assert!(
            matches!(&rows[0].content, Content::Text { text, .. } if text == "hello from history")
        );
        assert_eq!(archive.chats().unwrap()[0].name, "Fixture contact");
    }

    #[test]
    fn live_filter_accepts_selected_group_from_all_members_and_only_target_outbound() {
        let filter = DirectFilter {
            to: "target@s.whatsapp.net".into(),
            from: "team@g.us".into(),
        };
        assert!(accept_live_chat(&filter, "team@g.us", false));
        assert!(!accept_live_chat(&filter, "other@g.us", false));
        assert!(accept_live_chat(&filter, "target@s.whatsapp.net", true));
        assert!(!accept_live_chat(&filter, "unrelated@s.whatsapp.net", true));
    }

    #[test]
    fn unrelated_history_conversation_is_not_archived_or_emitted() {
        let mut conversation = wa::Conversation::default();
        conversation.id = "unrelated@s.whatsapp.net".into();
        conversation.name = Some("unrelated fixture".into());
        let archive = Archive::in_memory().unwrap();
        let (events, received) = std::sync::mpsc::channel();
        let filter = DirectFilter {
            to: "target@s.whatsapp.net".into(),
            from: "selected@g.us".into(),
        };
        store_history_conversation(
            &conversation,
            &filter,
            &archive,
            &events,
            &BackendWake::new(|| {}),
        );
        assert!(archive.chat("unrelated@s.whatsapp.net").unwrap().is_none());
        assert!(received.try_recv().is_err());
    }

    #[test]
    fn chat_metadata_event_contains_only_the_selected_receive_chat() {
        let archive = Archive::in_memory().unwrap();
        archive
            .ensure_chat("selected@g.us", "Selected group")
            .unwrap();
        archive
            .ensure_chat("unrelated@s.whatsapp.net", "Unrelated")
            .unwrap();
        let (events, received) = std::sync::mpsc::channel();
        emit_selected_chat(&archive, "selected@g.us", &events, &BackendWake::new(|| {}));
        assert!(
            matches!(received.recv().unwrap(), Event::ChatUpdated(chat) if chat.id == "selected@g.us")
        );
    }
}
