//! Simple external ID system with prefix and UUID bytes

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// External ID with prefix and UUID bytes
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ExternalId {
    pub prefix: String,
    pub bytes: [u8; 16],
}

#[allow(dead_code)]
impl ExternalId {
    /// Generate a new external ID with given prefix
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
            bytes: Uuid::new_v4().as_bytes().to_owned(),
        }
    }

    /// Convert to string representation: "prefix-{base36}"
    pub fn to_string(&self) -> String {
        let encoded = base36::encode(&self.bytes);
        format!("{}-{}", self.prefix, encoded)
    }

    /// Get the UUID
    pub fn uuid(&self) -> Uuid {
        Uuid::from_bytes(self.bytes)
    }
}

impl fmt::Display for ExternalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_external_id_creation() {
        let id = ExternalId::new("aid");
        assert_eq!(id.prefix, "aid");
        assert!(id.to_string().starts_with("aid-"));
    }

    #[test]
    fn test_uuid_roundtrip() {
        let id = ExternalId::new("task");
        let uuid = id.uuid();
        let reconstructed = Uuid::from_bytes(id.bytes);
        assert_eq!(uuid, reconstructed);
    }
}
