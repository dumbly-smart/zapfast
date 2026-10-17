# ZapFast SSH Direct Conversation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend the existing `zapfast-ssh` client so one command selects a recipient and one conversation, loads only that conversation, supports groups, and sends/receives text there.

**Architecture:** Keep the existing SSH profile, vault, protocol connection, and terminal lifecycle. Resolve `--to` and `--from` against contact/chat metadata, initialize the TUI on `--from`, and filter live/history storage and search to that chat (plus outgoing messages to `--to`) before writes.

**Tech Stack:** Rust 1.98, existing SSH feature and backend, SQLCipher archive, ratatui/crossterm, whatsapp-rust.

**Spec:** [SSH direct session design](../specs/2026-10-17-ssh-client-design.md)

## Global Constraints

- Linux and Rust 1.98 are the initial target; desktop remains the default build.
- Reuse the pinned `oxidezap/whatsapp-rust` source; do not implement protocol behavior here.
- Keep per-Unix-user profile isolation, the existing encrypted archive, and passphrase unlock behavior.
- Never log contact names, phone numbers, message bodies, QR payloads, session secrets, passphrases, or archive keys.
- Do not enumerate the chat list in direct mode. Read messages and search results only for `--from`; request older history only for that chat and only after user action.
- Store only selected `--from` inbound messages and `--to` outgoing messages in this SSH mode; a group `--from` accepts messages from every member of that group.
- Use synthetic fixtures and temporary archives only; never inspect a real user's archive contents.

## Review Focus

- Missing, malformed, unknown, or ambiguous selectors fail clearly and never silently choose a contact/group; Task 1 tests each case.
- Startup loads only `--from`, marks only `--from` read, and older-history requests cannot name another chat; Task 2 tests command scope.
- History-sync and live events from unrelated chats never reach the archive or terminal; Task 3 tests before-write filtering.
- Group selectors resolve by exact subject, group messages from different members are accepted, and sends can target groups; Tasks 1–3 test these paths.
- Existing QR pairing, passphrase unlock, terminal cleanup, SSH artifact, and desktop behavior remain intact; Task 4 runs the SSH and desktop checks.

---

### Task 1: Parse and resolve contact/group selectors

**Files:**
- Create: `src/ssh/selector.rs`
- Modify: `src/ssh/mod.rs`, `src/bin/zapfast-ssh.rs`, `src/archive.rs`
- Test: selector and archive unit tests

**Interfaces:**
- `ChatSelector::parse(value: &str) -> anyhow::Result<ChatSelector>` accepts an international phone number, canonical WhatsApp JID, or exact contact/group name.
- `ResolvedChat { id: ChatId, name: String, is_group: bool }` carries one resolved conversation.
- `Archive::chat_names(name: &str) -> Result<Vec<ChatNameMatch>>` returns exact contact full/first/push-name and group-subject matches from metadata only; it never reads message rows. `ChatNameMatch { id: String, name: String, is_group: bool }` is defined in `archive.rs`.
- `zapfast-ssh` requires `--to VALUE` and `--from VALUE`, then passes parsed selectors to `ssh::run`.

- [ ] **Step 1: Write failing resolver tests.** Cover number/JID parsing, exact full/first/push-name matching, exact group-subject matching, missing names, duplicate names, and a name collision across contacts and groups.
- [ ] **Step 2: Run focused selector/archive tests and confirm the new resolver is missing.**
- [ ] **Step 3: Implement selector parsing and metadata-only matching.** Use targeted SQL over contact and group metadata; do not query message rows. Return every exact candidate so ambiguity is explicit.
- [ ] **Step 4: Write failing CLI parser tests.** Cover both required flags, missing values, duplicate flags, unknown flags, and malformed selector input before passphrase prompting or terminal entry.
- [ ] **Step 5: Implement argument parsing and wire parsed selectors through `ssh::run`.** Keep `--help` and existing passphrase redaction behavior.
- [ ] **Step 6: Run selector, archive, and CLI tests; commit `feat: select SSH chats by number or name`; push `feat/ssh-client`.**

### Task 2: Open only the selected conversation in the terminal

**Files:**
- Modify: `src/ssh/state.rs`, `src/ssh/render.rs`, `src/ssh/mod.rs`
- Test: SSH state and render tests

**Interfaces:**
- `SshState::new(to: ResolvedChat, from: ResolvedChat) -> SshState` starts with `open_chat = from.id` and retains `to.id` as the send destination.
- `SshState::initial_commands(&self) -> Vec<Command>` requests `Command::LoadChat` for the newest page of `from` and marks only `from` read.
- `SshState::apply(&mut self, event: Event)` accepts messages/history only for `from`; a group `from` accepts any sender within that chat.
- `SshState::send_text(&self, text: String) -> Command` sends through `to`; older-history and search commands always name `from`.

- [ ] **Step 1: Write failing state tests.** Cover initial load and `MarkRead` for `from` only, rejection of events from unrelated chats, multiple group members in the same group, older-history request scoped to `from`, chat-scoped search, and outgoing `SendText` scoped to `to`.
- [ ] **Step 2: Run focused state tests and verify they fail on the current browse-first state.**
- [ ] **Step 3: Implement direct-session state.** Keep only the selected `from` transcript in memory; load its newest archived page at startup, request older phone history only after explicit user action, and scope all search to `from`.
- [ ] **Step 4: Write failing render tests.** Cover headers showing resolved send/receive chats and no unrelated chat list.
- [ ] **Step 5: Implement direct-session rendering and input flow.** Keep existing QR, connection, composer, older-page, search, and quit behavior.
- [ ] **Step 6: Run all SSH state/render tests; commit `feat: open one selected SSH conversation`; push `feat/ssh-client`.**

### Task 3: Filter history and live messages before archive writes

**Files:**
- Modify: `src/ssh/backend.rs`, `src/ssh/mod.rs`
- Test: SSH backend tests

**Interfaces:**
- `DirectFilter { to: ChatId, from: ChatId }` is passed into the SSH backend before protocol events are processed.
- `process_wa_event` and `store_history_conversation` receive the filter. History sync stores/emits only `from`; inbound live messages are accepted only for `from`; outbound messages may be stored only for `to`.
- Group filtering uses the group chat ID, so messages from every group member are accepted without a sender allowlist.
- `Command::Search { chat, query }` uses the existing `Archive::search_chat_messages` and emits results only for `chat`.

- [ ] **Step 1: Write failing backend tests.** Cover history sync containing selected and unrelated chats, live messages from selected and unrelated chats, two members in one selected group, outbound messages to `to`, chat-scoped search, and `Event::Chats` containing only the selected receive chat.
- [ ] **Step 2: Run focused backend tests and verify unrelated rows are currently archived/emitted and global search is used.**
- [ ] **Step 3: Implement filtering before `Archive::insert_message` and terminal event emission.** Retain only `from` history and selected inbound live rows; retain outgoing `to` rows; never expose unrelated chat metadata/messages in direct mode.
- [ ] **Step 4: Write a failing pagination test.** Verify `FetchOlder` names only `from` and is sent only after explicit user action, never automatically at startup.
- [ ] **Step 5: Run all SSH backend tests; commit `fix: restrict SSH session to selected chats`; push `feat/ssh-client`.**

### Task 4: Operator guide and full build verification

**Files:**
- Modify: `docs/ssh.md`, `src/bin/zapfast-ssh.rs` help text if needed
- Test: full SSH test suite and build checks

- [ ] **Step 1: Update `docs/ssh.md`.** Document `ssh -t host zapfast-ssh --to "NAME" --from "NAME"`, number/JID selectors, exact contact/group names, ambiguity errors, loading/searching only `--from`, explicit older paging, group send/receive, pairing, archive passphrase, and logout.
- [ ] **Step 2: Verify the SSH test suite.** Run `CARGO_TARGET_DIR=/home/xtrmn8/zapfast/target cargo test --no-default-features --features ssh --lib --offline`.
- [ ] **Step 3: Verify the SSH release target.** Run `CARGO_TARGET_DIR=/home/xtrmn8/zapfast/target cargo build --release --no-default-features --features ssh --bin zapfast-ssh --offline`; inspect dependency tree and binary size.
- [ ] **Step 4: Verify desktop and dependency policy.** Run `CARGO_TARGET_DIR=/home/xtrmn8/zapfast/target cargo check --features desktop --bin zapfast --offline` and `.github/scripts/check-dependency-sources.py`.
- [ ] **Step 5: Commit `docs: explain direct SSH conversations and groups`; push `feat/ssh-client`.**

## Final review

- [ ] Trace selectors and selected-chat behavior against the spec: number/JID, contact/group names, unique/missing/ambiguous matches, direct/group sends, and one-chat history/search.
- [ ] Confirm unrelated history-sync/live messages are rejected before archive writes and terminal events.
- [ ] Confirm only `--from` is loaded and marked read, and explicit older history is scoped to it.
- [ ] Confirm QR linking, wrong-passphrase file preservation, terminal restoration, SSH dependency exclusions, desktop build, and dependency-source policy remain intact.
- [ ] Record unavailable live WhatsApp testing; use only synthetic profiles and fixture archives.
