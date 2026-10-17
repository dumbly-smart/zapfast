use anyhow::{Context, bail};
use zapfast::ssh::selector::ChatSelector;

fn parse_args(args: impl IntoIterator<Item = String>) -> anyhow::Result<Option<(ChatSelector, ChatSelector)>> {
    let mut to = None;
    let mut from = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" { return Ok(None); }
        let slot = match arg.as_str() { "--to" => &mut to, "--from" => &mut from, _ => bail!("unknown argument {arg:?}; use --help") };
        if slot.is_some() { bail!("{arg} may only be supplied once"); }
        let value = args.next().with_context(|| format!("{arg} requires a value"))?;
        *slot = Some(ChatSelector::parse(&value).with_context(|| format!("invalid value for {arg}"))?);
    }
    Ok(Some((to.context("--to is required")?, from.context("--from is required")?)))
}

fn main() -> anyhow::Result<()> {
    let parsed = parse_args(std::env::args().skip(1))?;
    let Some((to, from)) = parsed else {
        println!("zapfast-ssh — headless WhatsApp client for SSH terminals\n\nUsage: ssh -t host zapfast-ssh --to <number|JID|name> --from <number|JID|name>\n\nThe archive passphrase is read interactively and is never accepted in arguments or environment variables.");
        return Ok(());
    };

    zapfast::ssh::current_tty()?;
    let passphrase = zapfast::ssh::terminal::read_passphrase()?;
    let profile = zapfast::ssh::profile::Profile::discover()
        .context("prepare the per-user ZapFast SSH profile")?;
    zapfast::ssh::run(profile, passphrase, to, from)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(items: &[&str]) -> Vec<String> { items.iter().map(|s| (*s).to_owned()).collect() }
    #[test]
    fn parses_required_flags_and_rejects_bad_invocations() {
        let parsed = parse_args(args(&["--to", "+14155550123", "--from", "Design Team"])).unwrap().unwrap();
        assert_eq!(parsed.0, ChatSelector::parse("+14155550123").unwrap());
        assert_eq!(parsed.1, ChatSelector::Name("Design Team".into()));
        for bad in [&[][..], &["--to", "1"][..], &["--from", "2"][..], &["--to"][..], &["--to", "1", "--to", "2", "--from", "3"][..], &["--wat", "x"][..], &["--to", "@g.us", "--from", "2"][..]] {
            assert!(parse_args(args(bad)).is_err(), "accepted {bad:?}");
        }
        assert!(parse_args(args(&["--help"])).unwrap().is_none());
    }
}
