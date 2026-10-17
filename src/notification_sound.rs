use serde::{Deserialize, Serialize};

/// The sound a new-message notification makes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationSound {
    /// Pidgin's message sound, the default for new messages.
    #[default]
    #[serde(alias = "chime")]
    Receive,
    /// Pidgin's alert sound, the default for mentions and replies to us.
    #[serde(alias = "ripple")]
    Alert,
    /// Whatever the operating system plays for notifications.
    System,
    /// No sound.
    None,
    /// An audio file ZapFast plays itself.
    Custom(std::path::PathBuf),
}
