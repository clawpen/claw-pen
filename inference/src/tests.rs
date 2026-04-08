#[cfg(test)]
mod tests {
    use crate::model::{ModelLoader, SamplingParams, GenerateRequest};
    use crate::inference::InferenceEngine;
    use crate::api::{ChatCompletionRequest, ModelsResponse, ModelInfo};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_model_loader_creation() {
        let loader = ModelLoader::new("test.gguf");
        assert!(!loader.is_loaded().await);
    }

    #[tokio::test]
    async fn test_sampling_params_default() {
        let params = SamplingParams::default();
        assert_eq!(params.temperature, 0.7);
        assert_eq!(params.top_p, Some(0.9));
        assert_eq!(params.top_k, Some(40));
    }

    #[tokio::test]
    async fn test_inference_engine_creation() {
        let model = Arc::new(ModelLoader::new("test.gguf"));
        let _engine = InferenceEngine::new(model);
        // Engine should be created successfully
    }

    #[tokio::test]
    async fn test_chat_completion_request_parsing() {
        let json = r#"{
            "model": "qwen3.5-4b",
            "messages": [
                {"role": "user", "content": "Hello"}
            ],
            "temperature": 0.7,
            "max_tokens": 100
        }"#;

        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "qwen3.5-4b");
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.messages[0].content, "Hello");
        assert_eq!(req.temperature, Some(0.7));
        assert_eq!(req.max_tokens, Some(100));
    }

    #[tokio::test]
    async fn test_models_response_format() {
        let response = ModelsResponse {
            object: "list".to_string(),
            data: vec![ModelInfo {
                id: "qwen3.5-4b".to_string(),
                object: "model".to_string(),
                owned_by: "claw-pen-inference".to_string(),
                permissions: vec![],
            }],
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("qwen3.5-4b"));
        assert!(json.contains("list"));
    }

    #[tokio::test]
    async fn test_sampling_params_with_float_types() {
        let params = SamplingParams {
            temperature: 0.5,
            top_p: Some(0.8),
            top_k: Some(30),
            repeat_penalty: 1.1,
            repeat_last_n: 32,
        };
        assert_eq!(params.temperature, 0.5);
        assert_eq!(params.top_p, Some(0.8));
        assert_eq!(params.top_k, Some(30));
    }
}
