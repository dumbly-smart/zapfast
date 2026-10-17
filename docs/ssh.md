# ZapFast over SSH

`zapfast-ssh` is a small, headless Rust terminal client for the everyday text
workflow: link WhatsApp, browse chats and local history, search archived text,
and send or reply to messages. WhatsApp protocol work is handled by the pinned
`whatsapp-rust` dependency. Media, calls, notifications, and desktop UI are not
included in this binary.

## Build and install

Install the Rust toolchain and the native build prerequisites for the
`whatsapp-rust` SQLite/OpenSSL stack. From a checkout of this repository:

```sh
cargo build --release --no-default-features --features ssh --bin zapfast-ssh
install -m 0755 target/release/zapfast-ssh "$HOME/.local/bin/zapfast-ssh"
```

On the server, ensure that directory is in the login user's `PATH`. Each Unix
account gets a separate profile under `${XDG_STATE_HOME:-$HOME/.local/state}/zapfast-ssh/`.
The directory is private to that user. A kernel lock prevents two sessions for
the same Unix user from opening the profile at once.

## Connect

Use normal SSH key authentication and allocate a terminal:

```sh
ssh -t user@server zapfast-ssh
```

The client prompts for the archive passphrase without echo. It asks for this on
each run. On first launch, use the pairing QR code from WhatsApp on your phone
(Linked devices → Link a device). Keep the terminal open while using the client;
logging out stops the connection. No ZapFast listener or background service is
started.

Use arrow keys or `j`/`k` to move through chats and messages, Enter to open a
chat, `/` to search, `i` to compose, Enter to send, `r` to reply to the selected
message, Page Up to request older history, Esc to go back, and `q` to quit.
Older history depends on what WhatsApp makes available to linked devices.

The profile contains the WhatsApp device session, encrypted SQLCipher archive,
and `archive.key`, a passphrase-wrapped random archive key. Keep the passphrase
safe; ZapFast does not upload it or message content. Back up the profile only
while the client is stopped, and preserve all files together.

For a live link or message smoke check, use a dedicated test WhatsApp account.
Never use a real user's archive as a test fixture.

## Build footprint

On Linux x86_64, the release command above produced a 21,831,072-byte binary
in this checkout. `ldd` reported only `libc`, `libm`, and `libgcc_s` (plus the
system dynamic loader); the normal SSH dependency tree has no GUI, audio, video,
or fastframe crates. The exact size can vary with the Rust toolchain and target.
