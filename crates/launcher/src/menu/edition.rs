//! Persistent connection routing. Omitted edition fields remain Bedrock.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerEdition {
    #[default]
    Bedrock,
    Java,
}
impl ServerEdition {
    pub fn default_port(self) -> u16 {
        match self {
            Self::Bedrock => super::DEFAULT_PORT.parse().expect("Bedrock default port"),
            Self::Java => java_protocol::status::DEFAULT_PORT,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Bedrock => "Bedrock Edition",
            Self::Java => "Java Edition",
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn existing_servers_remain_bedrock_and_java_round_trips() {
        let old: super::super::SavedServer =
            serde_json::from_str(r#"{"name":"old","address":"localhost"}"#).unwrap();
        assert_eq!(old.edition, ServerEdition::Bedrock);
        let java = super::super::SavedServer {
            edition: ServerEdition::Java,
            ..old
        };
        let round: super::super::SavedServer =
            serde_json::from_slice(&serde_json::to_vec(&java).unwrap()).unwrap();
        assert_eq!(round.edition, ServerEdition::Java);
        assert_ne!(
            ServerEdition::Java.default_port(),
            ServerEdition::Bedrock.default_port()
        );
    }
}
