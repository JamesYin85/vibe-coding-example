use crate::capability::Capability;
use crate::capability::CapabilityInfo;
use crate::error::{AgentError, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, trace, warn, instrument};

pub struct CapabilityRegistry {
    capabilities: HashMap<String, Arc<dyn Capability>>,
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        debug!("Creating new capability registry");
        Self {
            capabilities: HashMap::new(),
        }
    }

    #[instrument(skip(self, capability), fields(capability_name = %capability.name()))]
    pub fn register(&mut self, capability: Arc<dyn Capability>) {
        let name = capability.name().to_string();
        info!(capability = %name, description = %capability.description(), "Registering capability");

        if self.capabilities.contains_key(&name) {
            debug!(capability = %name, "Overwriting existing capability");
        }

        self.capabilities.insert(name, capability);
    }

    #[instrument(skip(self))]
    pub fn unregister(&mut self, name: &str) -> Option<Arc<dyn Capability>> {
        debug!(capability = %name, "Unregistering capability");
        self.capabilities.remove(name)
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Capability>> {
        self.capabilities.get(name).cloned()
    }

    pub fn has(&self, name: &str) -> bool {
        self.capabilities.contains_key(name)
    }

    pub fn list(&self) -> Vec<CapabilityInfo> {
        trace!(count = self.capabilities.len(), "Listing capabilities");
        self.capabilities
            .values()
            .map(|c| CapabilityInfo::from_capability(c.as_ref()))
            .collect()
    }

    #[instrument(skip(self, input), fields(capability = %name))]
    pub async fn execute(&self, name: &str, input: Value) -> Result<Value> {
        debug!(capability = %name, "Executing capability");

        let capability = self.get(name).ok_or_else(|| {
            warn!(capability = %name, available = ?self.capabilities.keys().collect::<Vec<_>>(), "Capability not found");
            AgentError::capability_not_found(name)
        })?;

        match capability.execute(input).await {
            Ok(result) => {
                debug!(capability = %name, "Capability executed successfully");
                Ok(result)
            }
            Err(e) => {
                warn!(capability = %name, error = %e, "Capability execution failed");
                Err(e)
            }
        }
    }

    pub fn len(&self) -> usize {
        self.capabilities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct TestCapability;

    #[async_trait]
    impl Capability for TestCapability {
        fn name(&self) -> &str {
            "test"
        }

        fn description(&self) -> &str {
            "A test capability"
        }

        async fn execute(&self, input: Value) -> Result<Value> {
            Ok(input)
        }
    }

    #[tokio::test]
    async fn test_register_and_execute() {
        let mut registry = CapabilityRegistry::new();
        registry.register(Arc::new(TestCapability));

        assert!(registry.has("test"));
        assert_eq!(registry.len(), 1);

        let input = serde_json::json!({"key": "value"});
        let result = registry.execute("test", input.clone()).await.unwrap();
        assert_eq!(result, input);
    }

    #[tokio::test]
    async fn test_capability_not_found() {
        let registry = CapabilityRegistry::new();
        let result = registry.execute("nonexistent", Value::Null).await;
        assert!(result.is_err());
    }
}
