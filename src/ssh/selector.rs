use anyhow::{Result, bail};

use crate::{archive::{Archive, ChatNameMatch}, model::ChatId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatSelector {
    Id(ChatId),
    Name(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedChat {
    pub id: ChatId,
    pub name: String,
    pub is_group: bool,
}

impl ChatSelector {
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        if value.is_empty() { bail!("chat selector cannot be empty"); }
        if value.starts_with('+') && value[1..].chars().all(|c| c.is_ascii_digit()) && value.len() > 1 {
            return Ok(Self::Id(format!("{}@s.whatsapp.net", &value[1..])));
        }
        if value.starts_with('+') { bail!("international phone numbers must contain only digits after +"); }
        if let Some((user, server)) = value.split_once('@') {
            if !user.is_empty() && !server.is_empty() && !value.chars().any(char::is_whitespace) {
                return Ok(Self::Id(value.to_owned()));
            }
            bail!("invalid WhatsApp JID selector");
        }
        Ok(Self::Name(value.to_owned()))
    }

    pub fn resolve(&self, archive: &Archive) -> Result<ResolvedChat> {
        match self {
            Self::Id(id) => {
                if let Some(chat) = archive.chat(id)? {
                    Ok(ResolvedChat { id: chat.id, name: chat.name, is_group: chat.kind == crate::model::ChatKind::Group })
                } else {
                    let is_group = id.ends_with("@g.us");
                    Ok(ResolvedChat { id: id.clone(), name: id.clone(), is_group })
                }
            }
            Self::Name(name) => {
                let matches = archive.chat_names(name)?;
                let candidate = match matches.as_slice() {
                    [] => bail!("no contact or group named {name:?} in the local archive; use its phone number or JID"),
                    [candidate] => candidate,
                    _ => bail!("{name:?} matches multiple contacts or groups; use a phone number or JID"),
                };
                Ok(candidate.clone().into())
            }
        }
    }
}

impl From<ChatNameMatch> for ResolvedChat {
    fn from(value: ChatNameMatch) -> Self {
        Self { id: value.id, name: value.name, is_group: value.is_group }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_numbers_jids_and_names() {
        assert_eq!(ChatSelector::parse("+14155550123").unwrap(), ChatSelector::Id("14155550123@s.whatsapp.net".into()));
        assert_eq!(ChatSelector::parse("120363@g.us").unwrap(), ChatSelector::Id("120363@g.us".into()));
        assert_eq!(ChatSelector::parse("Design Team").unwrap(), ChatSelector::Name("Design Team".into()));
        assert!(ChatSelector::parse("+abc").is_err());
        assert!(ChatSelector::parse("@g.us").is_err());
        assert!(ChatSelector::parse(" ").is_err());
    }
}
