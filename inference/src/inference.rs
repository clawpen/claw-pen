use anyhow::Result;
use std::sync::Arc;
use crate::model::{ModelLoader, GenerateRequest, GenerateResponse};
use tokio::sync::Mutex;

/// Text generation engine using llama-gguf
///
/// The engine is loaded once and reused for all requests.
/// Generation requests are processed sequentially to avoid
/// concurrent access to the underlying model.
pub struct InferenceEngine {
    model: Arc<ModelLoader>,
    engine: Mutex<Option<llama_gguf::Engine>>,
    model_path: String,
}

impl InferenceEngine {
    pub fn new(model: Arc<ModelLoader>) -> Self {
        let model_path = model.model_path().to_string();
        Self {
            model,
            engine: Mutex::new(None),
            model_path,
        }
    }

    /// Ensure the engine is loaded
    async fn ensure_loaded(&self) -> Result<()> {
        let mut engine_guard = self.engine.lock().await;

        if engine_guard.is_none() {
            // Load the engine in a blocking task
            let model_path = self.model_path.clone();

            let loaded_engine = tokio::task::spawn_blocking(move || {
                use llama_gguf::{EngineConfig, Engine};

                // Enable GPU acceleration and cap context length
                // GPU mode properly supports max_context_len capping
                let config = EngineConfig {
                    model_path,
                    max_context_len: Some(4096), // Cap context - works with GPU!
                    use_gpu: true,                // Enable CUDA acceleration
                    temperature: 0.7,
                    ..Default::default()
                };

                Engine::load(config)
            })
            .await??;

            *engine_guard = Some(loaded_engine);
        }

        Ok(())
    }

    /// Generate text based on a prompt
    pub async fn generate(&self, request: GenerateRequest) -> Result<GenerateResponse> {
        // Ensure model metadata is loaded
        let config = self.model.load().await?;

        // Ensure engine is loaded
        self.ensure_loaded().await?;

        // Validate max_tokens
        let max_tokens = request.max_tokens.unwrap_or(config.max_tokens)
            .min(config.context_window);

        // Get the prompt
        let prompt = request.prompt;

        // Lock and use the engine for generation
        // We need to hold the lock for the entire generation to prevent concurrent access
        let mut engine_guard = self.engine.lock().await;

        // Take the engine out temporarily (we'll put it back after)
        let engine = engine_guard.take()
            .ok_or_else(|| anyhow::anyhow!("Engine not loaded"))?;

        // Run generation in blocking task
        let result = tokio::task::spawn_blocking(move || {
            // Generate text - note: llama-gguf's generate method takes the prompt by reference
            let text = engine.generate(&prompt, max_tokens)?;
            // Return both the text and the engine so we can reuse it
            Ok::<(_, llama_gguf::Engine), anyhow::Error>((text, engine))
        })
        .await??;

        // Put the engine back
        *engine_guard = Some(result.1);

        Ok(GenerateResponse {
            text: result.0,
            finish_reason: "length".to_string(),
            tokens_used: max_tokens,
        })
    }
}
