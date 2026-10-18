# Android Termux SSH Host Design

## Goal

Let an Android phone host the existing `zapfast-ssh` terminal client. Another
machine connects to the phone through OpenSSH and starts the WhatsApp TUI in
that SSH terminal. This replaces the Linux server for a single Termux user.

## User flow

1. Install a supported Termux build on the Android phone.
2. In Termux, check out ZapFast and run the repository's setup command. It
   installs the required Termux packages, builds the `ssh` Rust target on the
   phone, and installs `zapfast-ssh` in the Termux user's executable path.
3. Add the connecting machine's SSH public key to Termux's
   `~/.ssh/authorized_keys`. The setup must never request a private key or
   overwrite an existing `authorized_keys` file.
4. Enable Termux's OpenSSH service on its default port 8022. It remains
   available while Android permits Termux to run. Document Termux:Boot as an
   optional way to start the service after a reboot.
5. On the same Wi-Fi network, run
   `ssh -t -p 8022 <termux-user>@<phone-ip> zapfast-ssh` from another machine.
   A remote TTY is required. The first run creates a private profile, prompts
   for its archive passphrase, and shows WhatsApp pairing on the remote
   terminal. Later runs reuse that profile. Existing `--to` and `--from`
   selectors remain available.

Remote access outside the LAN can use a separate private network such as
Tailscale. Termux OpenSSH remains the SSH server; the guide must not claim that
Android provides Tailscale SSH server mode. The initial setup needs no public
port forwarding or hosted ZapFast service.

## Components

- **Termux setup script:** Detect a real Termux environment, install the
  documented build and SSH packages, build the existing `zapfast-ssh` target,
  install it in a path available to Termux SSH logins, and configure only the
  Termux OpenSSH service. Rerunning the script must preserve SSH keys, the
  WhatsApp session, the encrypted archive, and the passphrase vault.
- **Client compatibility:** Keep the same Rust `ssh` feature, encrypted
  archive, terminal UI, profile lock, and protocol backend. Make only the
  Android-specific code or build changes needed for this target; do not add an
  SSH protocol implementation or a second WhatsApp backend.
- **Guide:** Update `docs/ssh.md` and the README entry with the phone-host
  commands, key setup, port, finding the phone's LAN address, first pairing,
  restart after reboot, and connection troubleshooting. Retain the Linux
  server instructions.

## Session and data behavior

The OpenSSH daemon is the background service. `zapfast-ssh` starts only for
an interactive SSH login and disconnects from WhatsApp when that terminal
session ends. Its files stay in Termux's private home under the current SSH
profile path. The archive passphrase is entered at each run and is never put
in command arguments or an environment variable. The existing one-process
lock prevents simultaneous use of that profile. An unrooted Termux install
has one app Unix identity, so this phone setup offers one independent profile.

## Boundaries and failure handling

- The setup script must stop with a clear message outside Termux or when a
  required package/build step fails. It must not erase profile data.
- A failed build must leave any previously installed `zapfast-ssh` executable
  usable. Publish a finished binary into place only after compilation succeeds.
- Android may suspend or stop background Termux processes. Document the
  relevant Android battery settings and Termux:Boot option, without promising
  uninterrupted availability.
- Do not silently disable SSH authentication or rewrite the user's
  `sshd_config`. The recommended flow uses public-key authentication.
- This feature does not make the WhatsApp client itself an always-on daemon.

## Acceptance

- The repository provides an executable, repeatable Termux setup path and
  exact SSH command from another machine.
- The Android Rust target compiles in a supported Termux environment, or any
  remaining platform blocker is identified with a reproducible build command
  and a focused source change before claiming support.
- The Linux `ssh` target and existing profile behavior remain usable.
- Setup documentation explains what remains running after an SSH logout and
  after a phone reboot.
