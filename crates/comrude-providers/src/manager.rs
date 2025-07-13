use crate::traits::LLMProvider;
use comrude_core::{Config, GenerationRequest, GenerationResponse, Result, ProviderError};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct ProviderManager {
    providers: Arc<RwLock<HashMap<String, Box<dyn LLMProvider>>>>,
    current_provider: Arc<RwLock<Option<String>>>,
    current_models: Arc<RwLock<HashMap<String, String>>>, // provider_name -> model_name
    config: Arc<Config>,
}

impl ProviderManager {
    pub fn new(config: Config) -> Self {
        Self {
            providers: Arc::new(RwLock::new(HashMap::new())),
            current_provider: Arc::new(RwLock::new(None)),
            current_models: Arc::new(RwLock::new(HashMap::new())),
            config: Arc::new(config),
        }
    }

    pub async fn register_provider(&self, provider: Box<dyn LLMProvider>) -> Result<()> {
        let name = provider.name().to_string();
        let mut providers = self.providers.write().await;
        providers.insert(name, provider);
        Ok(())
    }

    pub async fn set_current_provider(&self, provider_name: &str) -> Result<()> {
        let providers = self.providers.read().await;
        if providers.contains_key(provider_name) {
            let mut current = self.current_provider.write().await;
            *current = Some(provider_name.to_string());
            Ok(())
        } else {
            Err(comrude_core::ComrudeError::Provider(
                ProviderError::NotFound(provider_name.to_string())
            ))
        }
    }

    pub async fn get_current_provider(&self) -> Result<Arc<dyn LLMProvider>> {
        let current = self.current_provider.read().await;
        if let Some(provider_name) = &*current {
            self.get_provider(provider_name).await
        } else {
            Err(comrude_core::ComrudeError::Provider(
                ProviderError::NotConfigured("No current provider set".to_string())
            ))
        }
    }

    pub async fn get_provider(&self, name: &str) -> Result<Arc<dyn LLMProvider>> {
        let providers = self.providers.read().await;
        if let Some(_) = providers.get(name) {
            // Note: This is a temporary workaround due to trait object limitations
            // In a real implementation, we'd need to restructure this to use Arc<dyn LLMProvider>
            Err(comrude_core::ComrudeError::Provider(
                ProviderError::NotFound("Provider access pattern needs refactoring".to_string())
            ))
        } else {
            Err(comrude_core::ComrudeError::Provider(
                ProviderError::NotFound(name.to_string())
            ))
        }
    }

    pub async fn list_providers(&self) -> Vec<String> {
        let providers = self.providers.read().await;
        providers.keys().cloned().collect()
    }

    pub async fn get_current_provider_name(&self) -> Option<String> {
        let current = self.current_provider.read().await;
        current.clone()
    }

    pub async fn generate(&self, mut request: GenerationRequest) -> Result<GenerationResponse> {
        // Check if a specific provider is requested in metadata
        let provider_name = if let Some(preferred) = request.metadata.get("preferred_provider") {
            preferred.as_str().map(|s| s.to_string())
        } else {
            let current = self.current_provider.read().await;
            current.clone()
        };

        let provider_name = provider_name.ok_or_else(|| {
            comrude_core::ComrudeError::Provider(
                ProviderError::NotConfigured("No provider specified".to_string())
            )
        })?;

        let providers = self.providers.read().await;
        let provider = providers.get(&provider_name).ok_or_else(|| {
            comrude_core::ComrudeError::Provider(
                ProviderError::NotFound(provider_name.clone())
            )
        })?;

        // Set default model from config if not specified
        if request.model.is_none() {
            // Check if there's a current model set for this provider
            let current_models = self.current_models.read().await;
            request.model = current_models.get(&provider_name)
                .cloned()
                .or_else(|| Some(self.get_default_model(&provider_name)));
        }

        provider.generate(request).await
    }

    pub async fn health_check(&self, provider_name: &str) -> Result<comrude_core::HealthStatus> {
        let providers = self.providers.read().await;
        let provider = providers.get(provider_name).ok_or_else(|| {
            comrude_core::ComrudeError::Provider(
                ProviderError::NotFound(provider_name.to_string())
            )
        })?;

        provider.health_check().await
    }

    pub async fn health_check_all(&self) -> HashMap<String, Result<comrude_core::HealthStatus>> {
        let providers = self.providers.read().await;
        let mut results = HashMap::new();

        for (name, provider) in providers.iter() {
            let health = provider.health_check().await;
            results.insert(name.clone(), health);
        }

        results
    }

    fn get_default_model(&self, provider_name: &str) -> String {
        match provider_name {
            "openai" => self.config.providers.openai
                .as_ref()
                .map(|c| c.default_model.clone())
                .unwrap_or_else(|| "gpt-4".to_string()),
            "anthropic" => self.config.providers.anthropic
                .as_ref()
                .map(|c| c.default_model.clone())
                .unwrap_or_else(|| "claude-3-5-sonnet-20241022".to_string()),
            "ollama" => self.config.providers.ollama
                .as_ref()
                .map(|c| c.default_model.clone())
                .unwrap_or_else(|| "codellama:7b".to_string()),
            _ => "unknown".to_string(),
        }
    }

    pub async fn auto_select_provider(&self) -> Result<String> {
        let enabled_providers = self.config.get_enabled_providers();
        
        if enabled_providers.is_empty() {
            return Err(comrude_core::ComrudeError::Provider(
                ProviderError::NotConfigured("No providers enabled".to_string())
            ));
        }

        // Prefer cloud providers over local ones for better reliability
        let preferred_order = ["anthropic", "openai", "ollama", "google", "huggingface"];
        
        for provider in preferred_order {
            if enabled_providers.contains(&provider.to_string()) {
                let providers = self.providers.read().await;
                if providers.contains_key(provider) {
                    return Ok(provider.to_string());
                }
            }
        }

        // Fallback to first available
        Ok(enabled_providers[0].clone())
    }

    pub async fn list_models_for_current_provider(&self) -> Result<Vec<comrude_core::ModelInfo>> {
        let current = self.current_provider.read().await;
        if let Some(provider_name) = &*current {
            self.list_models_for_provider(provider_name).await
        } else {
            Err(comrude_core::ComrudeError::Provider(
                ProviderError::NotConfigured("No current provider set".to_string())
            ))
        }
    }

    pub async fn list_models_for_provider(&self, provider_name: &str) -> Result<Vec<comrude_core::ModelInfo>> {
        let providers = self.providers.read().await;
        let provider = providers.get(provider_name).ok_or_else(|| {
            comrude_core::ComrudeError::Provider(
                ProviderError::NotFound(provider_name.to_string())
            )
        })?;

        provider.list_models().await
    }

    pub async fn set_model_for_current_provider(&self, model: &str) -> Result<()> {
        let current = self.current_provider.read().await;
        if let Some(provider_name) = &*current {
            let mut current_models = self.current_models.write().await;
            current_models.insert(provider_name.clone(), model.to_string());
            Ok(())
        } else {
            Err(comrude_core::ComrudeError::Provider(
                ProviderError::NotConfigured("No current provider set".to_string())
            ))
        }
    }

    pub async fn get_current_model(&self) -> Option<String> {
        let current = self.current_provider.read().await;
        if let Some(provider_name) = &*current {
            let current_models = self.current_models.read().await;
            current_models.get(provider_name).cloned()
                .or_else(|| Some(self.get_default_model(provider_name)))
        } else {
            None
        }
    }
}

impl Default for ProviderManager {
    fn default() -> Self {
        Self::new(Config::default())
    }
}