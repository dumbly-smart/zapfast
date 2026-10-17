# ZapFast SSH Direct Session Design

## Goal

Provide a small Rust WhatsApp client that runs inside a normal SSH terminal on
a Linux host. The user starts one focused conversation by choosing where to
send messages and which conversation to receive. The client loads that selected
conversation alone instead of browsing or loading every chat.

## User workflow

Run the client from an SSH terminal with a recipient and an incoming sender:

```sh
ssh -t host zapfast-ssh --to "Design Team" --from "Family"
```

Either value may be an international phone number, a WhatsApp group JID, an
exact contact name, or an exact group subject. Contact names resolve from
contact metadata (full name, first name, or push name); group names resolve
from chat metadata. A selector must have one exact match; a missing or
ambiguous name produces a clear error that asks for a number or group JID.
Selector resolution must not read message rows. Numbers and group JIDs remain
available when names are not present locally.

After archive unlock and selector resolution, the terminal opens a focused text
session. `--from` selects the one conversation whose history and incoming
messages are shown; `--to` selects the conversation that receives outgoing
messages. They may refer to the same or different conversations. Startup loads
the newest page from the local archive for `--from` only. Older history may be
requested by the user for that same conversation; never fetch older history
automatically or for another chat. When `--from` is a group, show messages
from all members of that group. Mark only the selected `--from` conversation
read while it is shown. A concise header shows both resolved selectors,
connection state, and send errors. The user can quit cleanly with a documented
key or command.

## Scope and loading behavior

- Do not enumerate or render the account's chat list.
- Query and display archived messages only for the selected `--from`
  conversation, in bounded pages. An explicit older-history request remains
  limited to that same conversation; never request history automatically.
- Search, when used, is scoped to the selected `--from` conversation; never
  run cross-chat message search in direct mode.
- Resolve contact and group names using contact/chat metadata only, not message
  rows.
- In the live worker, filter inbound messages before archive insertion and
  terminal events. Only messages in the selected `--from` conversation may be
  displayed or archived; for a group this includes all members. Outgoing
  messages to `--to` may be archived normally.
- Mark only the selected `--from` conversation read; do not send read receipts
  for any other chat.
- Filter initial history-sync rows to the selected `--from` conversation before
  archive writes; discard rows and chat metadata for unrelated conversations.
- Other WhatsApp protocol traffic still reaches the linked-device client as
  required by the protocol, but unrelated messages are not displayed or stored
  by this direct-session path.
- Keep the first version to text send and receive. Unsupported content gets a
  short placeholder only when it belongs to the selected incoming contact.
- The session remains online only while the SSH process is running. Do not add
  a listener, background service, or hosted backend.

## Access and account isolation

- The host's existing SSH service authenticates users and supplies a terminal.
  ZapFast opens no listening port and has no hosted backend or account service.
- Each Unix user runs with their own identity and private XDG config, state, and
  cache directories. Account data is never shared between Unix users.
- Permit one active ZapFast SSH process per Unix user; a kernel lock releases
  when the process exits or SSH disconnects.
- A fresh profile can link as a WhatsApp companion device with a terminal QR
  code. Later runs reuse that account's saved linked-device session.

## Archive unlock and privacy

- A headless host may have no desktop keyring. Prompt for the archive
  passphrase without echo on each run. Never accept it in command arguments or
  environment variables.
- A random SQLCipher archive key is stored only in a passphrase-encrypted local
  key file using reviewed Rust implementations of Argon2id and
  XChaCha20-Poly1305. Never persist the plaintext passphrase or unwrapped key.
- A wrong passphrase, missing key for an existing archive, or failed key
  verification stops startup without replacing or resetting archive data.
- Create directories with mode `0700` and private files with mode `0600` or
  stricter. Normal Unix ownership is the user boundary.
- Do not log contact names, phone numbers, message bodies, QR payloads, session
  secrets, passphrases, or archive keys. Do not read any real user's archived
  content during implementation or validation.

## Architecture

- Implement `zapfast-ssh` as a separate Rust build target with a terminal
  frontend. Reuse ZapFast's existing `whatsapp-rust` protocol, backend worker,
  archive, and domain types; do not add a second protocol implementation.
- Add a direct-session runtime adapter that accepts resolved `--to` and
  `--from` chat identifiers, suppresses history loading, filters live inbound
  messages before archive writes and UI events, and sends text through the
  existing backend command path.
- Add a narrow archive/backend selector resolver that reads contact and chat
  metadata only and returns either one canonical WhatsApp identifier, no
  match, or an ambiguity result. It matches full, first, and push contact
  names and group subjects exactly and must not read messages to resolve a
  name.
- Keep terminal input, drawing, and cleanup inside the SSH frontend. The
  terminal must restore its mode on normal exit, handled signals, and errors.
- Keep the desktop client as the default build and preserve its current
  behavior. The SSH target must exclude GUI, tray, desktop notifications,
  updater, audio, video, camera, and VoIP dependencies.

## Small build and deployment

- Build a Linux release executable named `zapfast-ssh` with the terminal
  frontend and shared text-chat runtime only.
- Strip and optimize the release executable. Record its byte size and runtime
  library dependencies in the operator guide; it must not require a desktop
  session or display server.
- Document SSH installation, first-time QR linking, passphrase unlock, direct
  number and contact-name selectors, live filtering, and per-user profiles.
- The linked account is offline after logout; no service manager or background
  process is required.

## Error handling and validation

- Reject missing `--to` or `--from` arguments, malformed numbers, unresolved
  selectors, and ambiguous names before entering the live terminal session.
- Show connection state and send failures without claiming failed sends
  succeeded. Keep failed outgoing text available for retry where practical.
- On exit or SSH disconnect, stop the backend, close the archive, release the
  account lock, and restore terminal settings.
- Unit tests use temporary profiles, synthetic contact/group metadata, and
  mock protocol events. Cover exact selector matching, ambiguity, missing
  names, number/JID resolution, loading only the selected conversation's
  history, search scoped to the selected conversation, inbound filtering
  before archive writes, group messages from all members, send routing to
  `--to`, and clean shutdown.
- Run a manual SSH smoke check with temporary Unix accounts and profiles.
  Live WhatsApp linking and messaging require a dedicated test account; report
  if unavailable.
- Verify the desktop build and dependency-source policy. Inspect the SSH
  dependency tree to confirm desktop/media libraries are absent.
