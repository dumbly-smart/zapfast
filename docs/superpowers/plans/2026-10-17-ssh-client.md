# ZapFast SSH Client Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a small Rust WhatsApp text client that each Unix user runs in a normal SSH terminal on a Linux server.

**Architecture:** A `zapfast-ssh` binary uses the existing backend worker, archive, and `whatsapp-rust` protocol path through a headless build feature. Its own terminal controller handles keyboard input and backend events. Each Unix user has a private profile, process lock, and passphrase-wrapped archive key.

**Tech Stack:** Rust 1.98, Cargo features, `whatsapp-rust`, SQLCipher, Argon2id, XChaCha20-Poly1305, ratatui, crossterm, Linux file locking.

**Spec:** [SSH client design](../specs/2026-10-17-ssh-client-design.md)

## Global Constraints

- Linux and Rust 1.98 are the initial target; desktop remains the default build.
- Use the pinned `oxidezap/whatsapp-rust` source and the existing dependency-source policy. Do not implement the WhatsApp protocol here.
- Each Unix identity gets its own profile and at most one active SSH process.
- The SSH build has no GUI, tray, notification, updater, audio, video, camera, or VoIP dependency in its normal target dependency tree.
- The archive stays SQLCipher encrypted. A wrong or missing key never resets, replaces, or silently migrates an existing archive.
- Never log message bodies, phone numbers, QR payloads, session secrets, passphrases, or archive keys.
- No code or validation reads a real user's archive contents; use temporary profiles and mock data.
- The client runs during the SSH session; no listener or background service is created.

## Review Focus

- A wrong passphrase for an existing archive must leave both archive and key file byte-for-byte unchanged (Task 4).
- A second login by the same Unix user must fail before opening the session database, while a different Unix user gets a separate profile (Task 3).
- A restrictive or malformed terminal must show a useful linking/error state and restore terminal settings (Task 6).
- A refused send must keep the composed text and show the reason without claiming delivery (Task 5).
- An SSH disconnect or handled interrupt must stop the backend and release the profile lock (Task 7).

## File Map

- `Cargo.toml`, `build.rs`, `src/lib.rs`: select desktop or SSH dependencies and modules.
- `src/model.rs`, `src/settings.rs`, `src/notification_sound.rs`, `src/paths.rs`: make core data types and per-user paths available without egui.
- `src/backend.rs`, `src/backend/worker.rs`, applicable `src/backend/worker/*`: share the worker through a generic wake handle and a supplied archive key while omitting unused desktop/media/call paths from the SSH build.
- `src/archive.rs`, `src/archive/encryption.rs`: open an archive with an already-unwrapped key while retaining the desktop keyring flow.
- `src/ssh/profile.rs`, `src/ssh/vault.rs`: private server profile, session lock, and passphrase-wrapped key.
- `src/ssh/state.rs`, `src/ssh/render.rs`, `src/ssh/terminal.rs`, `src/ssh/qr.rs`, `src/ssh/mod.rs`: terminal state, commands/events, drawing, QR linking, and lifecycle.
- `src/bin/zapfast-ssh.rs`, `docs/ssh.md`: executable and operator instructions.

---

### Task 1: Headless Cargo and domain boundary

**Files:** Modify `Cargo.toml`, `build.rs`, `src/lib.rs`, `src/model.rs`, `src/settings.rs`, `src/paths.rs`; create `src/notification_sound.rs` and a minimal `src/ssh/mod.rs`.

**Interfaces:** Produce the `ssh` Cargo feature and an initial headless module set containing model, paths, and notification sound; Task 2 adds archive and backend. Keep `desktop` as the default. Move `NotificationSound` to `crate::notification_sound::NotificationSound` and re-export it from `settings` in desktop builds. Keep the existing desktop binary behind `required-features = ["desktop"]`.

- [ ] **Step 1: Record failing build checks.** Run `cargo check --no-default-features --features ssh --lib` and `cargo tree -p zapfast --no-default-features --features ssh -e normal`; the current manifest has no `ssh` feature and unconditionally includes desktop modules.
- [ ] **Step 2: Implement the feature boundary.** Make desktop-only dependencies optional; move the domain sound type and app-owned model types needed by backend into core; gate `model::Action` and `paths` legacy eframe migration to desktop; gate `build.rs` translation compilation and the desktop modules in `lib.rs`. Keep the currently pinned protocol dependency, with `voip` enabled by the desktop feature only.
- [ ] **Step 3: Verify the core slice.** Run `cargo check --no-default-features --features ssh --lib`; expect success. Run `cargo check --features desktop --bin zapfast`; expect success. `cargo tree -p zapfast --no-default-features --features ssh -e normal` must omit `eframe`, `egui`, `winit`, `rodio`, `openh264`, and desktop fastframe crates.
- [ ] **Step 4: Commit.** Commit only this build boundary and its type moves.

### Task 2: Shared backend without a window

**Files:** Modify `src/backend.rs`, `src/backend/worker.rs`, the worker submodules selected by compiler errors, `src/archive.rs`, and `src/archive/encryption.rs`.

**Interfaces:** Produce `backend::BackendWake::new(f: impl Fn() + Send + Sync + 'static) -> Self` and `BackendWake::wake(&self)`. Produce `backend::ArchiveUnlock::{PlatformKeyring, Supplied(Zeroizing<[u8; 32]>)}` and `Backend::spawn_headless(dirs: AccountDirs, wake: BackendWake, unlock: ArchiveUnlock) -> Backend`. Preserve desktop `Backend::spawn(dirs, fastframe_shell::Waker)` and `Archive::open(path)`; expose `Archive::open_with_key(path, key)` only within the crate.

- [ ] **Step 1: Write failing backend tests.** `wake_notifies_receiver`: one `wake()` produces one channel signal. `supplied_key_opens_archive`: the fixture row count is readable. `wrong_supplied_key_preserves_archive`: opening fails and archive bytes match the before image. Keep the existing keyring tests.
- [ ] **Step 2: Run the focused tests.** Run `cargo test --no-default-features --features ssh --lib backend::tests` and `cargo test --lib archive::encryption::tests`; expect the new tests to fail before the interfaces exist.
- [ ] **Step 3: Add the wake and unlock seams.** Adapt the desktop waker into `BackendWake`; pass `ArchiveUnlock` into `worker::run`; select `Archive::open` or `Archive::open_with_key` before the worker connects. Feature-gate call, media download, desktop update, and other worker paths unavailable in the SSH target, preserving desktop behavior. Incoming media still becomes a text placeholder.
- [ ] **Step 4: Verify both builds.** Run the focused tests, `cargo check --no-default-features --features ssh --lib`, and `cargo check --features desktop --bin zapfast`; expect success. Inspect the SSH normal dependency tree for every forbidden family in Global Constraints.
- [ ] **Step 5: Commit.** Commit the shared backend seam.

### Task 3: Private Unix profile and session lock

**Files:** Create `src/ssh/profile.rs`; modify `src/ssh/mod.rs` and `Cargo.toml` for the Linux locking crate.

**Interfaces:** Produce `Profile::discover() -> anyhow::Result<Profile>`, `Profile::under(root: &Path) -> anyhow::Result<Profile>` for temporary tests, `Profile::acquire(&self) -> anyhow::Result<SessionLock>`, and `Profile::account_dirs(&self) -> AccountDirs`. `SessionLock` holds an advisory exclusive lock until drop. Use `AccountId::first()` inside each Unix profile.

- [ ] **Step 1: Write failing profile tests.** `private_profile_modes`: assert directory mode `0700` and lock mode `0600`. `second_lock_fails`: assert `acquire()` returns an error until the first lock drops. `distinct_profiles_do_not_conflict`: assert two temporary roots each lock. `unsafe_root_is_rejected`: assert world-accessible and symlinked roots fail. Check another UID with disposable users in Task 7.
- [ ] **Step 2: Run `cargo test --no-default-features --features ssh ssh::profile::tests`; expect failure.**
- [ ] **Step 3: Implement discovery and lock.** Use the caller's XDG directories with an error if a private home/profile cannot be found; create and verify ownership/permissions before opening account files. Set process umask `077` before the backend can create a session database. Use a kernel advisory lock on a private lock file; release through RAII.
- [ ] **Step 4: Run the focused tests and `cargo check --no-default-features --features ssh --lib`; expect success.**
- [ ] **Step 5: Commit.** Commit the profile isolation boundary.

### Task 4: Passphrase-wrapped archive key

**Files:** Create `src/ssh/vault.rs`; modify `Cargo.toml` and `src/ssh/mod.rs`.

**Interfaces:** Produce `Vault::open_or_create(path: &Path, archive_path: &Path, passphrase: &str) -> anyhow::Result<Zeroizing<[u8; 32]>>`. The versioned key file stores an Argon2id salt, XChaCha20-Poly1305 nonce, ciphertext, and KDF parameters. Use a random 32-byte archive key. Set Argon2id to 64 MiB memory, 3 iterations, and 1 lane for new files; enforce bounded parameters when reading a file.

- [ ] **Step 1: Write failing vault tests.** `key_reopens`: assert two calls with the same passphrase return equal keys. `wrong_passphrase_preserves_files`: assert failure and byte equality for vault/archive before and after. `missing_vault_preserves_archive`: assert failure with no new file. `malformed_vault_is_bounded`: assert truncated, oversized, and excessive-KDF inputs fail. `failed_create_leaves_no_archive`: assert a forced key-file write failure creates no archive.
- [ ] **Step 2: Run `cargo test --no-default-features --features ssh ssh::vault::tests`; expect failure.**
- [ ] **Step 3: Implement the vault.** Use vetted `argon2` and `chacha20poly1305` crates, `getrandom`, and `zeroize`; atomically create a private `0600` key file, sync and read it back before returning the key. Reject existing archive/key conflicts without resetting data.
- [ ] **Step 4: Run the focused tests and an encrypted `Archive::open_with_key` fixture test; expect success.**
- [ ] **Step 5: Commit.** Commit the vault independently of terminal input.

### Task 5: Terminal chat controller

**Files:** Create `src/ssh/state.rs`; modify `src/ssh/mod.rs`.

**Interfaces:** Produce `SshState::new() -> Self`, `SshState::apply(&mut self, event: Event)`, and `SshState::act(&mut self, action: InputAction) -> Vec<Command>`. `InputAction` covers select/open/back, older page, global search, compose, quote, send, and quit. Keep `ChatId` scoped to this Unix profile. Use `Command::LoadChat`, `FetchOlder`, `SearchMessages`, `SendText`, and `MarkRead` rather than archive reads from the view.

- [ ] **Step 1: Write failing state tests.** `open_chat_loads_and_marks_read`: assert `LoadChat` and `MarkRead` for the selected ID. `older_pages_stop_at_complete`: assert one prepend and no further request. `search_keeps_current_query`: assert local title filtering, `SearchMessages`, and ignored stale results. `quoted_send_uses_reply_target`: assert `SendText.quoting` matches the selected message. `refused_send_keeps_draft`: assert text and reason remain visible. `incoming_respects_open_chat`: assert only the matching transcript changes.
- [ ] **Step 2: Run `cargo test --no-default-features --features ssh ssh::state::tests`; expect failure.**
- [ ] **Step 3: Implement event reduction and command generation.** Store only visible chat/message pages and search results; map unsupported content to a display placeholder; track connection state and delivery via `Link`, `MessageUpdated`, and refusal events.
- [ ] **Step 4: Run the focused tests; expect success.**
- [ ] **Step 5: Commit.** Commit the controller.

### Task 6: Terminal input, drawing, and QR linking

**Files:** Create `src/ssh/render.rs`, `src/ssh/terminal.rs`, and `src/ssh/qr.rs`; modify `src/ssh/mod.rs` and `Cargo.toml`.

**Interfaces:** Produce `terminal::TerminalGuard::enter() -> anyhow::Result<Self>` with cleanup in `Drop`, `render::draw(frame: &mut ratatui::Frame, state: &SshState)`, and `qr::encode_cells(payload: &str, width: u16, height: u16) -> Result<Vec<String>, QrSizeError>`. Never log the payload. Input mapping: arrows or `j/k` navigate, Enter opens/sends, Esc returns, `/` searches, `i` composes, `r` quotes the selected message, PageUp loads older history, and `q` quits outside an input field.

- [ ] **Step 1: Write failing rendering/input tests.** `small_terminal_hides_qr_payload`: assert resize guidance and no payload text. `qr_has_quiet_zone`: assert outer cells are blank and the matrix matches the QR fixture. `unsupported_message_has_placeholder`: assert a visible label. `resize_keeps_selection`: assert selected IDs unchanged. `input_error_restores_terminal`: assert cleanup calls are observed through a fake terminal writer.
- [ ] **Step 2: Run `cargo test --no-default-features --features ssh ssh::`; expect failure.**
- [ ] **Step 3: Implement ratatui/crossterm views and input.** Draw link, list, transcript, search, status, and composer states. Use a blocking input reader and a channel to wake the main loop on keyboard, resize, backend, or handled signal; redraw only after those events.
- [ ] **Step 4: Run the focused tests and `cargo check --no-default-features --features ssh --lib`; expect success.**
- [ ] **Step 5: Commit.** Commit terminal interaction.

### Task 7: SSH executable and release handoff

**Files:** Create `src/bin/zapfast-ssh.rs` and `docs/ssh.md`; modify `Cargo.toml` and `src/ssh/mod.rs`.

**Interfaces:** Produce `ssh::run(profile: Profile, passphrase: Zeroizing<String>) -> anyhow::Result<()>` and a private `SessionBackend` trait with `start`, `send`, `poll`, and `shutdown` methods for a testable event loop. Startup acquires `SessionLock`, unwraps the archive key, enters terminal mode, starts `Backend::spawn_headless`, grants its startup permit, then drives the controller until quit/disconnect. Explicit shutdown stops backend before releasing the lock. The binary requires `ssh` feature and a TTY; the passphrase is read without echo and never accepted through argv or environment.

- [ ] **Step 1: Write failing lifecycle tests.** `startup_once`: assert one permit send. `every_exit_shuts_down`: for quit, input error, HUP, and TERM signals, assert one backend shutdown and reacquirable `SessionLock`. `no_tty_leaves_profile_untouched`: assert a readable error and no new files.
- [ ] **Step 2: Run `cargo test --no-default-features --features ssh ssh::tests`; expect failure.**
- [ ] **Step 3: Implement the executable and docs.** Document build/install, SSH key login, `ssh -t host zapfast-ssh`, first QR link, passphrase unlock, per-user profiles, logout behavior, and the need for a dedicated test account for live checks.
- [ ] **Step 4: Verify release behavior.** Run the focused tests, `cargo build --release --no-default-features --features ssh --bin zapfast-ssh`, and `cargo check --features desktop --bin zapfast`. Record `stat` byte size, `ldd` runtime libraries, and `cargo tree -p zapfast --no-default-features --features ssh -e normal` in the handoff. Run `.github/scripts/check-dependency-sources.py`. Smoke-check with temporary Unix users and profiles; run live linking/chat only with a dedicated test WhatsApp account.
- [ ] **Step 5: Commit.** Commit the executable, deployment guide, and any final fixes.

## Final review

- [ ] Compare every section of the spec against these tasks; record any unmet behavior before calling the feature done.
- [ ] Inspect the SSH dependency tree and release artifact for desktop/media libraries and report actual size.
- [ ] Confirm no code path or test read a real user's archive or logged sensitive content.
- [ ] Check the desktop build and the repository's dependency-source rule after the feature split.
