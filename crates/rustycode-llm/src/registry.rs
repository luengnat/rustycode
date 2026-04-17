use crate::provider_v2::{LLMProvider, ProviderError};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ProviderRegistry {
    providers: RwLock<HashMap<String, Arc<dyn LLMProvider>>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: RwLock::new(HashMap::new()),
        }
    }

    pub async fn register(&self, provider: Arc<dyn LLMProvider>) {
        self.providers
            .write()
            .await
            .insert(provider.name().to_string(), provider);
    }

    pub async fn get(&self, name: &str) -> Option<Arc<dyn LLMProvider>> {
        self.providers.read().await.get(name).cloned()
    }

    pub async fn contains(&self, name: &str) -> bool {
        self.providers.read().await.contains_key(name)
    }

    pub async fn list_providers(&self) -> Vec<String> {
        self.providers.read().await.keys().cloned().collect()
    }

    pub async fn list_available(&self) -> Vec<String> {
        let providers = self.providers.read().await;
        let mut available = Vec::new();

        for (name, provider) in providers.iter() {
            if provider.is_available().await {
                available.push(name.clone());
            }
        }

        available
    }

    pub async fn unregister(&self, name: &str) -> bool {
        self.providers.write().await.remove(name).is_some()
    }

    pub async fn count(&self) -> usize {
        self.providers.read().await.len()
    }

    pub async fn clear(&self) {
        self.providers.write().await.clear();
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ProviderRegistryBuilder {
    registry: ProviderRegistry,
}

impl ProviderRegistryBuilder {
    pub fn new() -> Self {
        Self {
            registry: ProviderRegistry::new(),
        }
    }

    pub async fn with_provider(self, provider: Arc<dyn LLMProvider>) -> Self {
        self.registry.register(provider).await;
        self
    }

    pub fn build(self) -> ProviderRegistry {
        self.registry
    }

    pub async fn try_with_provider(
        self,
        provider: Arc<dyn LLMProvider>,
    ) -> Result<Self, ProviderError> {
        self.registry.register(provider).await;
        Ok(self)
    }
}

impl Default for ProviderRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}
