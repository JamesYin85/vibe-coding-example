use crate::error::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[async_trait]
pub trait Capability: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    async fn execute(&self, input: Value) -> Result<Value>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityInfo {
    pub name: String,
    pub description: String,
}

impl CapabilityInfo {
    pub fn from_capability(cap: &dyn Capability) -> Self {
        Self {
            name: cap.name().to_string(),
            description: cap.description().to_string(),
        }
    }
}
