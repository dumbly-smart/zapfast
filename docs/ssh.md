# ZapFast over SSH

`zapfast-ssh` is a small, headless Rust terminal client for text conversations.
Choose one chat to send to and one chat to receive from on the command line.
It loads and searches only the receive chat, and keeps WhatsApp connected only
while you stay logged in. It does not include the desktop window, media, calls,
or a background service.

## Build and install

Build on the Linux server from a ZapFast checkout:

```sh
cargo build --release --no-default-features --features ssh --bin zapfast-ssh
install -Dm755 target/release/zapfast-ssh "$HOME/.local/bin/zapfast-ssh"
```

Make sure `$HOME/.local/bin` is in the server user's `PATH`. Each Unix login
gets a private profile under `${XDG_STATE_HOME:-$HOME/.local/state}/zapfast-ssh/`.
It contains that user's linked-device session, passphrase-wrapped archive key,
and encrypted message archive. Only one SSH session per Unix user can use the
profile at a time. Different Unix users have separate WhatsApp profiles.

## Start a conversation

Allocate a terminal and specify both destinations:

```sh
ssh -t user@server zapfast-ssh --to +14155550123 --from +14155550123
```

The first run asks for a new archive passphrase, then shows a pairing QR code.
On your phone, open **WhatsApp → Settings → Linked devices → Link a device** and
scan it. Keep the terminal open while chatting. Logging out or pressing `q`
disconnects the client; the WhatsApp session stays linked for the next run.

`--to` is where your messages go. `--from` is the one conversation whose
incoming messages, local history, and search results are used. The local
archive marks only `--from` as read.
They can be different. For example, send messages to a group while watching a
family chat:

```sh
ssh -t user@server 'zapfast-ssh --to "Design Team" --from "Family"'
```

Each selector can be an international phone number with `+`, a WhatsApp JID,
or an exact saved contact name or group subject from the SSH profile's local
archive. A JID looks like `14155550123@s.whatsapp.net` for a direct chat or
`120363012345678@g.us` for a group. When a selector contains spaces, quote
the whole remote command so the remote shell preserves the name as one value.
Names must match a locally saved contact or group subject; partial matches are
not used. If a name is missing or matches more than one contact/group, the
client reports the problem before entering the terminal. Use a phone number or
JID to disambiguate. For a new SSH profile, start with numbers or JIDs; names
become available after WhatsApp syncs the corresponding contact/group metadata.

Group chats work as either `--to` or `--from`. When `--from` is a group, the
transcript accepts messages from all its members and shows the member name when
WhatsApp provides one. Sending still targets only `--to`.

## Terminal controls

- `i`: compose; Enter sends to `--to`
- `/`: search messages in `--from`
- `j` / `k` or Down / Up: move through the transcript
- Page Up: load the next local page; after local history ends, Page Up requests
  older WhatsApp history for `--from`
- `r`: reply to the selected message when `--to` and `--from` are the same chat
- Esc: leave the current input mode / clear message selection
- `q`: quit and disconnect

Older WhatsApp history depends on what the linked device can retrieve. The
client does not load unrelated chats, search them, or mark them read in its
local archive.

## Passphrase and storage

The archive passphrase is prompted without echo on every run. It is never
accepted as a command-line argument or environment variable. Keep it safe: it
protects the random key for the encrypted local archive. The archive and linked
device session remain in the Unix user's private profile. Back up the profile
only while the client is stopped, and preserve its files together. The SSH
profile is separate from the desktop app's archive.

For a live message smoke test, use a dedicated test WhatsApp account. Never
copy a real user's archive into test fixtures.

## Build footprint

The SSH feature excludes the desktop UI, audio, video, calls, and fastframe
desktop dependencies. The release binary size depends on the Rust toolchain,
target, and linked system libraries.
