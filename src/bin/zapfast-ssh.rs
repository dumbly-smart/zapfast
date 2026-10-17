use anyhow::{Context, bail};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    if let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            println!(
                "zapfast-ssh — headless WhatsApp client for SSH terminals\n\nUsage: ssh -t host zapfast-ssh\n\nThe archive passphrase is read interactively and is never accepted in arguments or environment variables."
            );
            return Ok(());
        }
        bail!("zapfast-ssh takes no arguments; passphrases are prompted without echo");
    }

    zapfast::ssh::current_tty()?;
    let passphrase = zapfast::ssh::terminal::read_passphrase()?;
    let profile = zapfast::ssh::profile::Profile::discover()
        .context("prepare the per-user ZapFast SSH profile")?;
    zapfast::ssh::run(profile, passphrase)
}
