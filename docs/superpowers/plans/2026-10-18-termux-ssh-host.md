# Android Termux SSH Host Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make an Android phone running Termux the SSH host for the existing `zapfast-ssh` client.

**Architecture:** A Termux-only setup script installs native build tools and OpenSSH, builds the current Rust SSH target on the phone, and atomically installs its binary. The guide explains SSH key login, background `sshd`, and the remote terminal command. The WhatsApp process and encrypted profile keep their current session lifecycle.

**Tech Stack:** Termux packages, POSIX shell, Rust `ssh` feature, OpenSSH, `termux-services`.

**Spec:** [Android Termux SSH Host Design](../specs/2026-10-18-termux-ssh-host-design.md)

## Global Constraints

- Preserve the Linux server workflow and SSH client's existing profile, vault, protocol, and TUI behavior.
- Do not copy or replace dependency sources; use only approved Cargo sources.
- Never read user chat data or embed credentials in commands, environment variables, or documentation examples.
- A phone is not attached to this workspace, so do not claim on-device execution or live SSH connectivity from local compilation alone.
- The user asked for implementation, not tests; perform build and static checks without adding or running test suites.

## Review Focus

- Rerunning setup must preserve `authorized_keys`, `session.db`, `archive.db`, and the passphrase vault.
- A failed build must not replace the previously installed executable.
- Setup must reject a non-Termux environment before package operations.
- The remote command must allocate a TTY and target port 8022.
- The guide must distinguish the background SSH daemon from the WhatsApp process, which ends on logout.

---

### Task 1: Termux setup command

**Files:**
- Create: `packaging/termux/setup.sh`

**Interfaces:**
- Consumes: the repository's `ssh` Cargo feature and `zapfast-ssh` binary target.
- Produces: an executable shell script invoked from a ZapFast checkout inside Termux.

- [ ] **Step 1: Add a guarded setup script.** Check `PREFIX` and Termux executable paths before running `pkg`; install `git`, `rust`, `clang`, `make`, `pkg-config`, `openssl`, and `termux-services`, then `openssh` so its service integration is present; build with `cargo build --release --locked --no-default-features --features ssh --bin zapfast-ssh`; install a completed binary atomically under `$HOME/.local/bin/`; start or enable Termux's `sshd` service without rewriting SSH configuration or keys. Explain any shell restart needed for `termux-services`.
- [ ] **Step 2: Inspect the script with `sh -n` and `shellcheck`.** Confirm both accept it; do not execute package installation in this Linux workspace.
- [ ] **Step 3: Commit the setup script.**

### Task 2: Phone-host guide

**Files:**
- Modify: `docs/ssh.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: `packaging/termux/setup.sh` from Task 1.
- Produces: a complete same-Wi-Fi setup and usage guide, with optional private-network access.

- [ ] **Step 1: Document phone setup and access.** Cover a supported Termux install, checkout and setup command, public-key login, `whoami`, phone Wi-Fi address, `ssh -t -p 8022 ... zapfast-ssh`, QR pairing from the other machine, single Termux user profile, service startup after reboot, Android battery limits, and optional Tailscale network access using Termux OpenSSH.
- [ ] **Step 2: Correct the existing SSH guide's stale direct-only description.** Preserve Linux commands and explain browse mode, ten-chat paging, Archived folder, and optional `--to`/`--from` selectors.
- [ ] **Step 3: Review the documentation and build the Linux SSH target.** Run `cargo check --locked --no-default-features --features ssh --bin zapfast-ssh` and inspect command examples for exact paths and quoting; do not claim Android build success without an attached phone.
- [ ] **Step 4: Commit and push this branch.**
