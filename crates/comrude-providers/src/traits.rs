use async_trait::async_trait;
use comrude_core::{
    GenerationRequest, GenerationResponse, StreamChunk, ProviderCapabilities, 
    ModelInfo, HealthStatus, Result
};
use futures::Stream;
use std::pin::Pin;

#[async_trait]
pub trait LLMProvider: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn description(&self) -> &str;
    
    fn capabilities(&self) -> ProviderCapabilities;
    fn supported_models(&self) -> Vec<ModelInfo>;
    
    async fn health_check(&self) -> Result<HealthStatus>;
    async fn test_connection(&self) -> Result<()>;
    
    async fn generate(&self, request: GenerationRequest) -> Result<GenerationResponse>;
    async fn generate_stream(&self, request: GenerationRequest) -> Result<Pin<Box<dyn Stream<Item = Result<StreamChunk>> + Send>>>;
    
    async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        Err(comrude_core::ComrudeError::Provider(
            comrude_core::ProviderError::ApiError {
                provider: self.name().to_string(),
                message: "Embeddings not supported by this provider".to_string(),
            }
        ))
    }
    
    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        Ok(self.supported_models())
    }
}