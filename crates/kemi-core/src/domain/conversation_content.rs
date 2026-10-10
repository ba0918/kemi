use serde::{Serialize, ser::SerializeStruct};

use super::parts::Part;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationContent {
    Plain(String),
    Parts(Vec<Part>),
}

impl From<String> for ConversationContent {
    fn from(body: String) -> Self {
        Self::Plain(body)
    }
}
impl From<&str> for ConversationContent {
    fn from(body: &str) -> Self {
        Self::Plain(body.into())
    }
}
impl ConversationContent {
    pub fn plain(&self) -> Option<&str> {
        match self {
            Self::Plain(body) => Some(body),
            Self::Parts(_) => None,
        }
    }
    pub fn parts(&self) -> Option<&[Part]> {
        match self {
            Self::Plain(_) => None,
            Self::Parts(parts) => Some(parts),
        }
    }
}

impl Serialize for ConversationContent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut value = serializer.serialize_struct("ConversationContent", 2)?;
        match self {
            Self::Plain(body) => {
                value.serialize_field("body", body)?;
                value.serialize_field("parts", &Option::<()>::None)?;
            }
            Self::Parts(parts) => {
                value.serialize_field("body", &Option::<()>::None)?;
                value.serialize_field("parts", parts)?;
            }
        }
        value.end()
    }
}
