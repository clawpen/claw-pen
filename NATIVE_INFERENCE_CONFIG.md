# Native Inference Service Configuration

## Overview

The Claw Pen orchestrator now supports a built-in native inference service that runs GGUF models locally without external dependencies like LM Studio or Ollama.

## Configuration

Add the following to your `claw-pen.toml` config file:

```toml
[native_inference]
# Path to your GGUF model file
model_path = "F:/Models/qwen3.5-4b.gguf"

# Port for the inference API server (default: 8765)
port = 8765

# Maximum context window in tokens (default: 4096)
max_tokens = 4096

# Default sampling parameters
temperature = 0.7
top_p = 0.9
```

## API Endpoints

Once configured, the orchestrator provides the following endpoints:

### Get Inference Status
```bash
GET /api/inference/status
```

Response:
```json
{
  "enabled": true,
  "status": "running",
  "endpoint": "http://localhost:8765",
  "model": "qwen3.5-4b"
}
```

### Start Inference Service
```bash
POST /api/inference/start
```

### Stop Inference Service
```bash
POST /api/inference/stop
```

## Direct Inference API

The inference service also exposes OpenAI-compatible endpoints directly:

### List Models
```bash
GET http://localhost:8765/v1/models
```

### Chat Completions
```bash
POST http://localhost:8765/v1/chat/completions
Content-Type: application/json

{
  "model": "qwen3.5-4b",
  "messages": [
    {"role": "user", "content": "Hello!"}
  ],
  "temperature": 0.7,
  "max_tokens": 100
}
```

### Streaming Chat
```bash
POST http://localhost:8765/v1/chat/completions
Content-Type: application/json

{
  "model": "qwen3.5-4b",
  "messages": [
    {"role": "user", "content": "Tell me a story"}
  ],
  "stream": true
}
```

## Building the Inference Service

First, build the inference service binary:

```bash
cd claw-pen/inference
cargo build --release
```

The binary will be available at:
- Windows: `target/release/claw-pen-inference.exe`
- Linux/Mac: `target/release/claw-pen-inference`

## Running Standalone

You can also run the inference service standalone:

```bash
claw-pen-inference --model-path "path/to/model.gguf" --port 8765
```

## Supported Models

The inference service supports GGUF format models, including:
- Qwen 2/2.5/3.5 models
- LLaMA 2/3 models
- Mistral/Mixtral models
- Any other GGUF-compatible model

## Example Full Config

```toml
# claw-pen.toml
deployment_mode = "windows-wsl"
network_backend = "tailscale"
container_runtime = "docker"

[native_inference]
model_path = "F:/Models/qwen3.5-4b.gguf"
port = 8765
max_tokens = 4096
temperature = 0.7
top_p = 0.9
```

## Troubleshooting

### Model file not found
Make sure the model path is correct and the file exists. Use absolute paths for reliability.

### Port already in use
Change the port in the config if 8765 is already in use.

### Inference service won't start
1. Ensure the inference binary is built
2. Check the orchestrator logs for detailed error messages
3. Verify the model file is a valid GGUF format

### Health check fails
The service may still be starting up. Wait a few seconds and try again.
