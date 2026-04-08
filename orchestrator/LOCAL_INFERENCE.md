# Local LLM Inference for Claw Pen

Run local LLM inference with CUDA acceleration using llama.cpp.

## Prerequisites

- NVIDIA GPU with CUDA support (RTX 3060 or higher recommended)
- Docker with NVIDIA Container Toolkit
- ~8GB+ VRAM for 7B models (quantized)

## Building the Image

```bash
cd orchestrator
bash build-local-inference.sh
```

This will build the `clawpen-local-inference:cuda-latest` image.

## Downloading Models

Download GGUF models from HuggingFace:

**Recommended for RTX 3070 (8GB):**
- [Qwen3.5-9B-Instruct-Q4_K_M.gguf](https://huggingface.co/unsloth/Qwen3.5-9B-GGUF/resolve/main/Qwen3.5-9B-Instruct-Q4_K_M.gguf) (~6GB) - **Recommended**
- [Qwen2.5-7B-Instruct-Q4_K_M.gguf](https://huggingface.co/Qwen/Qwen2.5-7B-Instruct-GGUF/resolve/main/Qwen2.5-7B-Instruct-Q4_K_M.gguf) (~5GB)
- [Llama-3.2-3B-Instruct-Q4_K_M.gguf](https://huggingface.co/quantized/Llama-3.2-3B-Instruct-GGUF/resolve-main/Llama-3.2-3B-Instruct-Q4_K_M.gguf) (~2GB)
- [Phi-3-mini-4k-instruct-q4.gguf](https://huggingface.co/microsoft/Phi-3-mini-4k-instruct-GGUF/resolve/main/Phi-3-mini-4k-instruct-q4.gguf) (~2.5GB)

Place models in: `claw-pen/data/models/`

## Running the Server

```bash
# Create models directory
mkdir -p claw-pen/data/models

# Copy your .gguf model to claw-pen/data/models/

# Run the inference server
docker run -d \
  --name clawpen-inference \
  --gpus all \
  -p 8080:8080 \
  -v "F:/Software/Claw Pen/claw-pen/data/models:/app/models" \
  clawpen-local-inference:cuda-latest
```

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| MODEL_PATH | /app/models | Directory containing GGUF models |
| HOST | 0.0.0.0 | Server bind address |
| PORT | 8080 | Server port |
| CTX_SIZE | 8192 | Context window size |
| N_GPU_LAYERS | 99 | Number of layers to offload to GPU |
| N_PARALLEL | 4 | Number of parallel requests |

## Testing the Server

```bash
# Test completion endpoint
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "auto",
    "messages": [{"role": "user", "content": "Hello!"}],
    "stream": false
  }'
```

## Creating Agents with Local Inference

In the Claw Pen agent creator:
1. Select **Provider: Local (llama.cpp CUDA)**
2. Select a model (server uses the loaded model)
3. Create agent - it will connect to your local inference server

The agent will automatically use:
- Base URL: `http://host.docker.internal:8080/v1`
- Model: `gpt-4o` (server default, ignored by llama.cpp)

## Model Recommendations

| GPU VRAM | Model | File | Notes |
|----------|-------|------|-------|
| 6GB | Phi-3-mini | ~2.5GB | Fast, good for simple tasks |
| 8GB | Qwen2.5-7B Q4 | ~5GB | Best quality/size balance |
| 12GB+ | Llama-3.1-8B Q4 | ~6GB | Better reasoning |
| 16GB+ | Mixtral-8x7B Q4 | ~9GB | Excellent coding |

## Performance Tips

1. **Use Q4_K_M quantization** - Best quality/size ratio
2. **Set N_GPU_LAYERS=-1** to offload all layers to GPU
3. **Use smaller models** for faster responses
4. **Reduce CTX_SIZE** if you don't need long context (saves VRAM)
