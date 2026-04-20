use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FlowId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeviceId(pub Uuid);

impl DeviceId {
    /// UUID without dashes, e.g. `7ab193b59cf34155adc9b36cbd7dbb87`.
    pub fn to_hex(&self) -> String {
        self.0.simple().to_string()
    }
}

/// Identity of a node (graph node, not just a flow node).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub Uuid);

impl NodeId {
    pub fn new_random() -> Self {
        Self(Uuid::new_v4())
    }

    /// UUID without dashes, e.g. `7ab193b59cf34155adc9b36cbd7dbb87`.
    pub fn to_hex(&self) -> String {
        self.0.simple().to_string()
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for NodeId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.parse()?))
    }
}

/// Append-only revision identifier (UUID v4; future: ULID / UUID v7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RevisionId(pub Uuid);

impl RevisionId {
    pub fn new_random() -> Self {
        Self(Uuid::new_v4())
    }

    /// UUID without dashes, e.g. `7ab193b59cf34155adc9b36cbd7dbb87`.
    pub fn to_hex(&self) -> String {
        self.0.simple().to_string()
    }
}

impl std::fmt::Display for RevisionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for RevisionId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.parse()?))
    }
}

impl FlowId {
    pub fn new_random() -> Self {
        Self(Uuid::new_v4())
    }

    /// UUID without dashes, e.g. `7ab193b59cf34155adc9b36cbd7dbb87`.
    pub fn to_hex(&self) -> String {
        self.0.simple().to_string()
    }
}

impl std::fmt::Display for FlowId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for FlowId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.parse()?))
    }
}
