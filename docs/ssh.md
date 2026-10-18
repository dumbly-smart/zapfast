# ZapFast over SSH

`zapfast-ssh` is a Rust terminal client for WhatsApp text chats. It runs on
the machine you SSH into: a Linux server or an Android phone running Termux.
That machine holds its own linked-device session and encrypted local archive.
The SSH daemon can stay in the background; ZapFast connects to WhatsApp only
while its terminal session is running.

## Host it on an Android phone with Termux

Use a current [Termux installation](https://github.com/termux/termux-app#installation)
on Android 7 or newer. Install Termux and any Termux add-ons from the same
source. These commands run **inside Termux on the phone**, not on the computer
you will connect from:

```sh
pkg update
pkg install git
git clone --branch feat/ssh-client https://github.com/dumbly-smart/zapfast.git
cd zapfast
./packaging/termux/setup.sh
```

The setup script installs Termux's Rust build tools, `termux-services`, and
OpenSSH. It builds the existing `ssh` Cargo target on the phone, installs the
finished binary in `$PREFIX/bin`, and enables the `sshd` service. A Rust build
may take time and need substantial free storage. You can rerun setup after a
source update; it preserves your linked-device session, archive, passphrase
vault, SSH keys, and any working binary if the new build fails. The script
does not edit `sshd_config` or create an SSH password.

### Allow your computer to log in

On the **computer**, display an existing SSH public key. If you do not have
one, create one with `ssh-keygen -t ed25519` without overwriting an existing
key:

```sh
cat ~/.ssh/id_ed25519.pub
```

On the **phone in Termux**, paste that single public-key line into
`~/.ssh/authorized_keys`:

```sh
mkdir -p ~/.ssh
chmod 700 ~/.ssh
cat >> ~/.ssh/authorized_keys
```

Paste one public-key line, press Enter, then Ctrl-D. Then run:

```sh
chmod 600 ~/.ssh/authorized_keys
whoami
sv status sshd
```

Use the name printed by `whoami` as `TERMUX_USER`. Find the phone's local IP
address in Android's Wi-Fi network details. On the **computer**, while both
devices are on the same Wi-Fi network, connect with a terminal allocated:

```sh
ssh -t -p 8022 TERMUX_USER@PHONE_WIFI_IP zapfast-ssh
```

Termux OpenSSH uses port **8022** because Android does not let ordinary apps
bind port 22. The first ZapFast run asks for a new archive passphrase and shows
a pairing QR in the computer's terminal. Scan it with WhatsApp on the phone
through **Settings → Linked devices → Link a device**. Keep the SSH session
open while chatting. On later runs, enter the same archive passphrase; the
linked-device session is reused. If the QR is clipped, enlarge the computer's
terminal and it will redraw.

To connect from outside the local Wi-Fi network, install a private network
such as [Tailscale](https://tailscale.com/kb/1452/connect-to-devices) on the
phone and connecting machine, then use the phone's private-network address in
the same SSH command. **Termux OpenSSH** still handles the SSH login; Android
does not host Tailscale SSH's own server mode. No public port forwarding is
required for the local or private-network setup.

### Keep the SSH service available

The setup enables `sshd` through `termux-services`. Check it with
`sv status sshd`; after reopening Termux, `sv-enable sshd` enables it again
if necessary. Android may stop background apps to save power. Allow Termux to
run in the background in your phone's battery settings if the service stops
when the screen is off.

For SSH access after a reboot, optionally install
[Termux:Boot](https://github.com/termux/termux-boot#how-to-use) from the same
source as Termux and open it once. Create an executable
`~/.termux/boot/start-services` in Termux:

```sh
mkdir -p ~/.termux/boot
cat > ~/.termux/boot/start-services <<'EOF'
#!/data/data/com.termux/files/usr/bin/sh
. /data/data/com.termux/files/usr/etc/profile.d/start-services.sh
EOF
chmod 700 ~/.termux/boot/start-services
```

You may add `termux-wake-lock` before the `.` line if you choose to keep the
device awake; this uses more battery. Reboot availability depends on Android
allowing Termux:Boot to start. This boot script starts only the SSH service
supervisor. It does not start WhatsApp or unlock the archive.

## Host it on a Linux server

From a ZapFast checkout on the server:

```sh
cargo build --release --locked --no-default-features --features ssh --bin zapfast-ssh
install -Dm755 target/release/zapfast-ssh "$HOME/.local/bin/zapfast-ssh"
```

Make sure `$HOME/.local/bin` is in that Unix user's remote-shell `PATH`, then
connect with `ssh -t user@server zapfast-ssh`. The server uses its existing
SSH daemon; ZapFast does not listen for SSH connections itself. Different Unix
users have separate ZapFast profiles. An unrooted Termux installation has one
app Unix user and one ZapFast profile.

## Browse and chat

With no selectors, ZapFast initially loads ten active chats. Move down the
list to load more. Press `a` in the chat list to open **Archived**; archived
chat pages load in that folder. Press `a` again to return to active chats.
Press Enter to open a chat, Backspace to return to the list, `i` to compose,
and Enter to send. The `?` key shows help; `q` exits.

You can still start directly with one destination for sending and one for
receiving:

```sh
ssh -t -p 8022 TERMUX_USER@PHONE_WIFI_IP \
  'zapfast-ssh --to "Design Team" --from "Family"'
```

On a Linux server, omit `-p 8022` unless its SSH daemon uses that port.
`--to` is where outgoing messages go; `--from` is the only conversation whose
incoming messages, local history, and search are shown in direct mode. They
may be the same chat. Each selector accepts an international number with `+`,
a WhatsApp JID, an exact saved contact name, or an exact group subject. Quote
the whole remote command when names contain spaces. Names become available
after that SSH profile has synced the corresponding metadata; use numbers or
JIDs initially. A direct-chat JID looks like `14155550123@s.whatsapp.net`;
a group JID looks like `120363012345678@g.us`.

In an open chat, `/` searches that chat, `j`/`k` or arrow keys move the
selection, `r` replies when sending to the same chat, and Page Up loads older
local history before requesting older history from WhatsApp. Older history
depends on what the phone makes available. Group chats work as destinations
or receive chats. Non-text messages are shown as placeholders.

## Profile and troubleshooting

Each host Unix user gets a private profile under
`${XDG_STATE_HOME:-$HOME/.local/state}/zapfast-ssh/`. It contains the WhatsApp
linked-device session, the passphrase-wrapped archive key, and the encrypted
archive. Only one ZapFast SSH process can use that profile at a time. Back it
up only while ZapFast is stopped, and preserve all files together. The SSH
profile is separate from the desktop app's profile. Losing the passphrase
means the encrypted archive cannot be unlocked.

If login fails, first check `sv status sshd` on the phone, the phone's current
IP address, port 8022, and the key and `authorized_keys` permissions. If
ZapFast cannot be found after SSH login, rerun the Termux setup script; it
installs the binary in `$PREFIX/bin`. If the build fails, read the first Rust
compiler or native-library error; the previous installed binary is preserved.
For a live-message smoke check, use a dedicated test WhatsApp account rather
than copying a real archive into test fixtures.

The SSH build excludes the desktop window, audio, video, calls, and desktop
notification dependencies. Its binary size depends on the phone architecture,
Rust toolchain, and native libraries.
