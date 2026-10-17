# ZapFast SSH Client Design

## Goal

Provide a small Rust WhatsApp client that a person runs inside a normal SSH
terminal on a Linux server. Several people can use the same host through
separate Unix accounts. Each person links and uses their own WhatsApp account.
The client connects while that person's SSH session is running and stops when
the session ends.

## Access model

- The host's existing SSH service authenticates users and supplies a terminal.
  A user can log in and run `zapfast-ssh`, or run it directly with
  `ssh -t host zapfast-ssh`.
- ZapFast opens no listening port and has no hosted backend or account service.
- One active ZapFast SSH process is allowed per Unix user. A second launch
  reports that the user's account is already open. The lock releases if the
  process exits or the SSH connection drops.
- Each process runs with its caller's Unix identity and uses that caller's
  private XDG config, state, and cache directories. Account data is never
  shared across Unix users.
- A fresh server profile can link as a WhatsApp companion device from a QR
  code rendered in the terminal. The client retains the linked session in its
  per-user state for later SSH logins.

## First usable scope

- Show connection and linking state, including a terminal QR code and clear
  instructions if the terminal is too small to display it.
- Browse chats and unread counts, open a chat, page through stored history,
  and see incoming text messages while connected.
- Search chats and messages through the existing archive/backend capabilities.
- Compose and send text messages, including a quoted reply when supported by
  the existing backend command path.
- Show a readable placeholder for message types this client cannot display.
- Exit cleanly on a quit command, terminal error, or SSH disconnect.

Media playback and sending, calls, voice recording, polls, interactive
responses, desktop window features, and background synchronization are outside
this first version. The WhatsApp connection exists only during the interactive
SSH session.

## Architecture

The SSH executable is a separate Rust build target with a terminal frontend.
It shares ZapFast's `whatsapp-rust` protocol path, backend worker, archive, and
domain models. A focused runtime adapter translates terminal actions into the
existing backend `Command`s and applies backend `Event`s to terminal state.
The terminal frontend never opens a second protocol implementation or reads
WhatsApp data directly from another user's files.

The existing desktop `App::headless` path still uses egui and is not the basis
of this client. The shared backend boundary needs a terminal-compatible wake
mechanism, and the common runtime/domain code must be buildable without
desktop presentation services. Changes to those boundaries stay limited to
what the SSH client needs. The desktop client keeps its current behavior.

Terminal input, rendering, and cleanup live in the SSH frontend. Rendering is
event driven: keyboard input or backend events trigger a redraw; the process
does not spin when idle. Chat lists and transcripts are paged rather than
loading an entire archive at startup.

## Private storage and archive unlock

Each Unix user has their own session database, encrypted archive, caches, and
account lock under private directories. Create directories with mode `0700`
and private files with mode `0600` or stricter. Normal Unix ownership is the
boundary between users.

A headless host may have no desktop OS keyring. On each launch the client asks
for that user's archive passphrase without echoing it. A random SQLCipher
archive key is stored only in a passphrase-encrypted local key file, using
reviewed Rust implementations of Argon2id and XChaCha20-Poly1305. The app
does not persist the plaintext passphrase or unwrapped key and clears their
memory when possible. A wrong passphrase, missing key file for an
existing encrypted archive, or failed key verification stops startup without
replacing or resetting archive data. A new profile creates its key before
linking. The SSH client does not silently migrate a desktop keyring profile.

QR payloads, phone numbers, message bodies, session secrets, passphrases, and
archive keys are absent from shipped logs. Errors shown to a user are scoped
to that user's session.

## Small build and deployment

- Build a Linux release executable named `zapfast-ssh` with the terminal
  frontend and shared text-chat runtime. GUI, tray, desktop notification,
  update, audio, video, camera, and VoIP dependencies are excluded from this
  target's dependency graph and linked artifact.
- Strip and optimize the release artifact. Record its byte size and runtime
  library dependencies alongside the build instructions, so the size claim is
  measurable. The release artifact is a single executable plus normal system
  libraries; it does not require a desktop session or display server.
- Provide installation and first-link instructions for a Linux host with
  working `sshd`, including per-user setup and the direct SSH command above.
- A user's account is offline after logout. No service manager or background
  process is required for the first version.

## Error handling and validation

- Terminal mode and screen state are restored on normal exit and handled
  interrupts. Backend shutdown releases the account lock and closes storage.
- Connection loss is shown in the terminal; reconnect behavior follows the
  existing backend. A failed send reports failure rather than claiming success.
- Unit validation covers terminal navigation, per-user paths, duplicate
  session locking, paging, passphrase failure, and command/event mapping with
  fixtures or mocks.
- A manual smoke run uses disposable Unix users and test profiles to check
  SSH disconnect/reconnect, file ownership and modes, and that one user's data
  is inaccessible through another user's client. Live linking and text chat
  require a dedicated test WhatsApp account; report that limit if unavailable.
- No validation step reads any real user's archive contents.
