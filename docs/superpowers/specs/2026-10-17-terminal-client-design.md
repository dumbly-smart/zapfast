# ZapFast Terminal Client Design

## Goal

Add a Rust terminal interface for everyday WhatsApp use that reuses ZapFast's
existing WhatsApp protocol implementation, linked accounts, and local archive.
The first usable version covers chat browsing, message history, search, and
sending messages.

## User and constraints

- The terminal client is for someone using ZapFast on the same machine as its
  existing account data.
- It uses the same account roster, per-account session and encrypted archive,
  settings, and cache directories as the desktop client.
- WhatsApp protocol behavior stays in `whatsapp-rust`; the terminal interface
  sends the same backend commands as the desktop interface and consumes backend
  events.
- Message content and credentials remain local and are never sent to a new
  service.
- The terminal frontend is implemented in Rust and runs in a text terminal.

## Scope

### Initial workflows

- Start the terminal interface from ZapFast's existing executable using a
  terminal-mode option.
- Show linked accounts and allow the user to choose the active account.
- Browse chats, display unread state, open a chat, and page through its archived
  message history.
- Search chats and messages using existing backend/archive capabilities.
- Compose and send text messages, including ordinary reply/quote behavior when
  available through existing commands.
- Show connection state and actionable errors without exposing message contents
  in logs.
- Exit cleanly, restoring terminal settings and shutting down the account
  backend.

### Not in the first version

Calls, media playback or sending, voice recording, polls, interactive message
responses, chat/account administration, and desktop window/tray behaviors are
not part of this first milestone. They can be considered after the text
workflows are usable.

## Architecture

ZapFast keeps one protocol/backend implementation. Startup selects either the
existing egui frontend or a new Rust terminal frontend. The terminal frontend
does not instantiate the desktop `App`, because `App::new` owns window-, tray-,
audio-, and theme-related process services. It instead uses the existing
account/runtime and backend command/event boundary, with a terminal-focused
presentation state for chat lists, open-chat history, search, and composition.

The terminal event loop owns terminal setup and rendering. It translates user
input into existing `model::Action`s or backend `Command`s through a narrow
controller, applies backend `Event`s to its presentation state, and redraws on
input or backend activity. It must not issue protocol requests directly or
mutate worker state. A wake/poll adapter may be added at the backend boundary
where the current window waker is too specific to support terminal redraws.

The terminal mode reads and writes the same account folders and encrypted
archive as desktop mode. Only one ZapFast mode may own those accounts at a time;
mode selection happens at process startup, and the existing single-instance
guard remains in force. Users exit the running mode before starting the other
mode. No account data is copied, migrated, or reset to enable the TUI.

## User-visible behavior

- The chat list and transcript are navigable entirely from the keyboard.
- The selected account is visible, and account-specific chat/message IDs remain
  scoped to that account.
- Live backend events update connection state, unread counts, and the open
  transcript without polling in a busy loop.
- Long transcripts and chat lists are paged or windowed; startup does not load
  an unbounded archive into memory.
- Errors such as an unavailable terminal, locked/missing archive key, or failed
  connection are shown in the terminal and do not silently switch to a
  disposable archive.

## Reliability and privacy

- Terminal raw mode and alternate-screen state are restored on normal exit,
  errors, and handled interrupts.
- Shutdown stops the account backend before releasing terminal state.
- No message bodies, phone numbers, keys, or QR payloads are added to shipped
  logs.
- Existing archive encryption, migrations, account isolation, and history
  preservation rules remain unchanged.

## Validation

- Unit tests cover terminal navigation state, paging, account scoping, and
  command generation without connecting to WhatsApp or reading user data.
- Backend integration tests use existing mocks/fixtures to verify that chat
  loading, history, search, send, event handling, and shutdown use the shared
  command/event path.
- A manual smoke run in a disposable test profile verifies terminal startup,
  navigation, clean exit, and successful subsequent desktop startup.
- No test or smoke check reads the user's actual archive content.

## Open implementation details

The implementation plan will decide the terminal UI crate and exact backend
wake adapter after checking the repository's dependency constraints and
existing command/event APIs. These choices must preserve the architecture and
scope above.
