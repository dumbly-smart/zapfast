<p align="center">
  <img src="docs/assets/images/logo.svg" alt="ZapFast logo" width="88" height="88">
</p>

<h1 align="center">ZapFast</h1>

<p align="center"><strong>WhatsApp, native and fast.</strong><br>A lightweight WhatsApp app for Linux, macOS, and Windows.</p>

<p align="center">
  <a href="https://zapfast.rocks/download/"><strong>Download</strong></a> ·
  <a href="https://zapfast.rocks/getting-started/">Getting started</a> ·
  <a href="https://zapfast.rocks/using-zapfast/">User guide</a> ·
  <a href="CONTRIBUTING.md">Contribute</a>
</p>

<p align="center">
  <a href="Cargo.toml"><img src="https://img.shields.io/badge/Built_with-Rust-00a884" alt="Built with Rust"></a>
  <a href="https://zapfast.rocks/download/"><img src="https://img.shields.io/badge/Linux_%C2%B7_macOS_%C2%B7_Windows-24292f" alt="Linux, macOS, and Windows"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-00a884" alt="MIT license"></a>
</p>

ZapFast is a WhatsApp client written in Rust with
[egui](https://www.egui.rs) and [fastframe](https://fastframe.dev), the
framework behind a family of native desktop apps. It links to your phone as a
companion device through
[whatsapp-rust](https://github.com/oxidezap/whatsapp-rust) and has no browser
engine. In our Linux test it opens in under a second and uses about 200 MB of
idle RAM, against 1.13 GB for WhatsApp Web.
[See the measurements](https://zapfast.rocks/benchmarks/).

The interface is available in English, Chinese (Simplified and Traditional),
French, German, Indonesian, Italian, Portuguese (Brazil), Russian, Spanish,
and Turkish.

<picture>
  <source media="(prefers-color-scheme: light)" srcset="docs/screenshot-light.png">
  <img src="docs/screenshot.png" alt="ZapFast showing a conversation with an attachment, voice messages, reactions, a quoted reply, and a link preview">
</picture>

<details>
<summary><strong>Watch ZapFast in action</strong></summary>

https://github.com/user-attachments/assets/2bf86b54-45fc-4add-8de7-c426c3cdad9b

</details>

[Features](#your-chats-on-your-desktop) · [Install](#install) · [Link your phone](#link-your-phone) · [Use over SSH](#use-zapfast-over-ssh) · [Help](#guides-and-help)

## Your chats, on your desktop

| Feature | What you can do |
|---|---|
| **Messages** | Send formatted text, replies, edits, reactions, forwards, pictures, files, stickers, GIFs, polls, and voice messages. |
| **Media in the chat** | Play voice messages, videos, round video messages, GIFs, and animated stickers in place. |
| **Your history** | Keep your chats in an encrypted archive on this computer, and fetch older messages from your phone as you scroll. |
| **Several numbers** | Link more than one WhatsApp number and switch between them in one window. |
| **Desktop controls** | Use keyboard shortcuts, select and copy text across messages, and stay linked in the tray with notifications. |
| **SSH terminal** | Run a small headless Rust client on a Linux server or Android phone with Termux. Browse chats and history, search, and send or reply to text messages. |
| **Themes** | Choose light, dark, system appearance, or custom colours. On Omarchy, follow your desktop theme. |
| **Privacy** | Hide locked chats behind a code, or the whole window behind an app lock. Nothing goes to a server of ours. |

## Install

| Platform | Installation |
|---|---|
| **macOS** | `brew install --cask crmne/tap/zapfast`, or [download the Mac app](https://zapfast.rocks/download/#macos). |
| **Arch Linux** | `yay -S zapfast-bin` |
| **Windows** | Choose your build on the [Download page](https://zapfast.rocks/download/#windows). |
| **Other Linux** | Find Nix, AppImage, and other options on the [Download page](https://zapfast.rocks/download/#linux). |
| **From source** | Follow [Getting started](https://zapfast.rocks/getting-started/) for dependencies and commands. |

## Link your phone

1. Open ZapFast. It shows a QR code.
2. On your phone, open WhatsApp, then **Settings**, **Linked devices**,
   **Link a device**, and scan the code. Or click **Link with phone number**
   and enter the eight-character code on your phone.
3. Your recent chats arrive from the phone. Older messages follow as you
   scroll up.

The link survives restarts, and your phone does not need to stay online to
read what ZapFast has already stored. See
[Getting started](https://zapfast.rocks/getting-started/) for the full
walkthrough and [How it links](https://zapfast.rocks/how-it-links/) for the
details.

## Use ZapFast over SSH

`zapfast-ssh` is a small headless text client for a Linux server or an Android
phone running Termux. The host's SSH service accepts connections; ZapFast
connects to WhatsApp only while your SSH terminal is open. By default it loads
ten active chats, loads more as you scroll, and loads archived chat pages when
you open Archived. You can also select one send and receive chat with `--to`
and `--from`. It supports direct chats and groups, without a desktop window,
media, or calls.

To make the **phone** the SSH host, run this inside Termux:

```sh
pkg install git
git clone https://github.com/dumbly-smart/zapfast.git
cd zapfast
if [ ! -f packaging/termux/setup.sh ]; then
  git fetch origin feat/ssh-client
  git switch -c feat/ssh-client FETCH_HEAD
fi
./packaging/termux/setup.sh
```

Add your computer's SSH public key to Termux's `~/.ssh/authorized_keys`, then
connect from that computer on the same Wi-Fi network. Replace `TERMUX_USER`
with Termux's `whoami` output and `PHONE_WIFI_IP` with the phone's Wi-Fi IP:

```sh
ssh -t -p 8022 TERMUX_USER@PHONE_WIFI_IP zapfast-ssh
```

The phone's OpenSSH service stays in the background; the WhatsApp session ends
when you leave the TUI. [The SSH guide](docs/ssh.md) covers SSH keys, pairing,
auto-start after reboot, and optional access over a private network.

To use a **Linux server** instead, build and install from a ZapFast checkout:

```sh
cargo build --release --locked --no-default-features --features ssh --bin zapfast-ssh
install -Dm755 target/release/zapfast-ssh "$HOME/.local/bin/zapfast-ssh"
```

Make sure `$HOME/.local/bin` is in that account's `PATH`, then connect from
your computer with a terminal allocated:

```sh
ssh -t your-user@your-server zapfast-ssh
```

Optional `--to` and `--from` selectors may be an international phone number,
WhatsApp JID, or exact contact/group name saved in the SSH profile. On a new
profile, use numbers or JIDs until WhatsApp syncs the local names. To send to
one group while reading another chat, quote the complete remote command so
the names stay single arguments:

```sh
ssh -t your-user@your-server 'zapfast-ssh --to "Design Team" --from "Family"'
```

The client prompts for your archive passphrase without displaying it. You
enter it each time you connect. On the first run, scan the QR code with
WhatsApp on your phone: **Settings → Linked devices → Link a device**. If the
terminal is too small to display the code, enlarge it; the QR display redraws
when the terminal resizes.

In browse mode, use **j/k** or **↑/↓** to move through chats, **Enter** to
open one, and **a** to switch between Chats and Archived. In an open chat,
**/** searches it, **i** composes, **Enter** sends, and **Page Up** loads older
local history before requesting older WhatsApp history. **r** replies when
sending to the same chat; **q** quits. Older history depends on what WhatsApp
provides to linked devices.

Each host Unix user has a private profile containing its WhatsApp session and
encrypted local archive; an unrooted Termux installation has one such user.
Only one ZapFast session per Unix user can run at a time. Keep the passphrase
safe; it is never accepted as a command-line argument or environment variable.
For storage details and troubleshooting, see the [SSH client guide](docs/ssh.md).

## Guides and help

The complete guide lives at **[zapfast.rocks](https://zapfast.rocks/)**.

| You want to… | Read |
|---|---|
| Know what ZapFast does and does not do yet | [What is ZapFast?](https://zapfast.rocks/what-is-zapfast/) |
| Link your phone or build from source | [Getting started](https://zapfast.rocks/getting-started/) |
| Run the headless terminal client over SSH | [SSH client guide](docs/ssh.md) |
| Learn messages, stickers, polls, and shortcuts | [Using ZapFast](https://zapfast.rocks/using-zapfast/) |
| Find settings, stored files, encryption, the app lock, and updates | [Settings and files](https://zapfast.rocks/settings-and-files/) |
| Make your own theme | [Making a theme](https://zapfast.rocks/themes/) |
| Understand what stays on this computer | [How it links](https://zapfast.rocks/how-it-links/) |
| Understand account risk | [Account safety](https://zapfast.rocks/what-is-zapfast/#account-safety) |
| Report a bug or propose a feature | Read [Contributing](CONTRIBUTING.md), then use the [issue forms](https://github.com/crmne/zapfast/issues/new/choose). |

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening an issue or pull
request. To look at the interface without a WhatsApp account, run:

```sh
cargo run --features demo -- --demo
```

[AGENTS.md](AGENTS.md) describes the architecture, [DEMO.md](DEMO.md) the
demo pages, screenshots, and recorded tours, and
[PACKAGING.md](PACKAGING.md) releases and packages.

Native notifications use Fastframe on Linux, macOS and Windows. ZapFast keeps
its account/chat routing, privacy settings, notification limits and sound
previews. Clicks open the announced message; reading a chat withdraws its
tracked notifications. OS permissions and notification presentation still apply.


## More native apps

**Want Spotify just as fast and native?** [Spotifast](https://spotifast.rocks)
is ZapFast's sibling. Both are built on
[fastframe](https://fastframe.dev).

## Acknowledgements

ZapFast uses [whatsapp-rust](https://github.com/oxidezap/whatsapp-rust),
[egui](https://www.egui.rs), [fastframe](https://fastframe.dev), the
[Inter](https://rsms.me/inter/) typeface and Noto Color Emoji (OFL), and
[Lucide](https://lucide.dev) icons (ISC), also used for the chat wallpaper
doodles. The notification sounds are [Pidgin](https://pidgin.im)'s, under the
GPL-2.0 (see `assets/sounds/`).

ZapFast is an unofficial client and is not affiliated with WhatsApp or Meta.
Using an unofficial client may be against WhatsApp's terms of service and
could get an account suspended. Use it at your own risk. WhatsApp is a
trademark of WhatsApp LLC.

Licensed under the [MIT License](LICENSE).

## Star history

<a href="https://www.star-history.com/?repos=crmne%2Fzapfast&amp;type=date">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=crmne/zapfast&amp;type=date&amp;theme=dark">
    <img alt="ZapFast GitHub stars over time" src="https://api.star-history.com/chart?repos=crmne/zapfast&amp;type=date">
  </picture>
</a>
