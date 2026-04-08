use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// GGUF model loader for quantized LLMs
pub struct ModelLoader {
    model_path: String,
    model: Arc<RwLock<Option<LoadedModel>>>,
}

struct LoadedModel {
    config: ModelConfig,
}

#[derive(Clone)]
pub struct ModelConfig {
    pub name: String,
    pub context_window: usize,
    pub vocab_size: usize,
    pub max_tokens: usize,
}

impl ModelLoader {
    pub fn new(model_path: &str) -> Self {
        Self {
            model_path: model_path.to_string(),
            model: Arc::new(RwLock::new(None)),
        }
    }

    /// Load the GGUF model (lazy loading on first use)
    pub async fn load(&self) -> Result<ModelConfig> {
        // Check if already loaded
        {
            let model = self.model.read().await;
            if let Some(ref loaded) = *model {
                return Ok(loaded.config.clone());
            }
        }

        // Check if model file exists
        if !Path::new(&self.model_path).exists() {
            anyhow::bail!("Model file not found: {}", self.model_path);
        }

        // Load the model using llama-gguf
        let model_path = self.model_path.clone();
        let config = tokio::task::spawn_blocking(move || {
            Self::load_model_config(&model_path)
        })
        .await??;

        // Store the loaded model
        *self.model.write().await = Some(LoadedModel {
            config: config.clone(),
        });

        Ok(config)
    }

    /// Load model configuration from GGUF file
    fn load_model_config(path: &str) -> Result<ModelConfig> {
        use llama_gguf::GgufFile;

        let gguf_model = GgufFile::open(path)?;

        // Extract metadata from the GGUF file
        let context_window = gguf_model
            .data
            .get_u64("llama.context_length")
            .unwrap_or(32768) as usize;

        let vocab_size = gguf_model
            .data
            .get_u64("llama.vocab_size")
            .unwrap_or(151936) as usize;

        // Get model name from path or metadata
        let name_from_meta = gguf_model
            .data
            .get_string("general.name")
            .map(|s| s.to_string());

        let name = name_from_meta.unwrap_or_else(|| {
            Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string()
        });

        Ok(ModelConfig {
            name,
            context_window,
            vocab_size,
            max_tokens: 4096,
        })
    }

    /// Check if model is loaded
    pub async fn is_loaded(&self) -> bool {
        self.model.read().await.is_some()
    }

    /// Get the model path
    pub fn model_path(&self) -> &str {
        &self.model_path
    }
}

/// Sampling parameters for text generation
#[derive(Clone, Debug)]
pub struct SamplingParams {
    pub temperature: f32,
    pub top_p: Option<f32>,
    pub top_k: Option<usize>,
    pub repeat_penalty: f32,
    pub repeat_last_n: usize,
}

impl Default for SamplingParams {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            top_p: Some(0.9),
            top_k: Some(40),
            repeat_penalty: 1.0,
            repeat_last_n: 64,
        }
    }
}

/// Text generation request
pub struct GenerateRequest {
    pub prompt: String,
    pub sampling: SamplingParams,
    pub max_tokens: Option<usize>,
}

/// Text generation response
pub struct GenerateResponse {
    pub text: String,
    pub finish_reason: String,
    pub tokens_used: usize,
}
