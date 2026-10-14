//! Credential-free progress labels; no URLs, keys, account names or payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Starting,
    Authentication,
    Fields,
    Membership,
    Tickets,
    Indexing,
}
impl Stage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Authentication => "CLI authentication",
            Self::Fields => "field metadata",
            Self::Membership => "board membership",
            Self::Tickets => "ticket pages + Mine",
            Self::Indexing => "indexing",
        }
    }
}
