# Performance Optimization Guide

Optimizing Claw Pen for local LLM inference with llama.cpp and CUDA.

## GPU Configuration

### RTX 3070 (8GB VRAM)

**Recommended Settings for 8B Q4 Model:**

```bash
./llama-server.exe \
  -m model.gguf \
  --port 8081 \
  --host 0.0.0.0 \
  --ctx-size 8192 \
  --n-gpu-layers 99 \
  --threads 8 \
  --batch-size 512 \
  --ubatch-size 128
```

### Parameters Explained

| Parameter | Value | Purpose |
|-----------|-------|---------|
| `--n-gpu-layers` | 99 | Offload all layers to GPU |
| `--ctx-size` | 8192 | Context window (tokens) |
| `--threads` | 8 | CPU threads (use physical cores) |
| `--batch-size` | 512 | Max batch size for processing |
| `--ubatch-size` | 128 | Micro batch size (VRAM tradeoff) |

## Monitoring Performance

### GPU Utilization

```bash
# Windows
nvidia-smi -l 1

# Check VRAM usage
nvidia-smi --query-gpu=memory.used,memory.total --format=csv
```

### llama.cpp Stats

The server reports timing with each response:
- `prompt_ms`: Time to process prompt
- `prompt_per_second`: Tokens/second for prompt
- `predicted_ms`: Time to generate response
- `predicted_per_second`: Tokens/second generation

## Tuning Guidelines

### For Faster Responses

1. **Reduce context size** (`--ctx-size 4096`)
2. **Lower batch size** (`--batch-size 256`)
3. **Use smaller model** (7B instead of 13B)

### For Better Quality

1. **Use higher quantization** (Q5_K_M instead of Q4_K_M)
2. **Increase context size** (`--ctx-size 16384`)
3. **Enable repetition penalty** (`--repeat-penalty 1.1`)

### For Multiple Concurrent Users

1. **Increase batch size** (`--batch-size 1024`)
2. **Reduce u-batch size** (`--ubatch-size 64`)
3. **Add CPU threads** (`--threads 16`)

## Model Selection

### Model Size vs VRAM

| Model | Quantization | VRAM Required | Speed |
|-------|--------------|---------------|-------|
| 8B | Q4_K_M | ~5GB | Fast |
| 8B | Q5_K_M | ~6GB | Medium |
| 8B | Q8_0 | ~8GB | Slow |
| 13B | Q4_K_M | ~8GB | Medium |
| 13B | Q4_K_M | ~10GB | Slow |

### Recommended Models for RTX 3070

1. **Llama-3.3-8B-Instruct Q4_K_M** - Best balance
2. **Llama-3.1-8B-Instruct Q5_K_M** - Better quality
3. **Mistral-7B-Instruct Q4_K_M** - Faster alternative

## System Optimization

### Windows

1. **Set GPU power limit** (optional):
   ```bash
   nvidia-smi -pl 220  # 220W for RTX 3070
   ```

2. **Disable Windows Game DVR** for VRAM savings

3. **Close VRAM-hungry apps** (browsers with many tabs)

### Docker

1. **Limit Docker memory** in settings to prevent host swapping

2. **Use WSL2 backend** for better performance

## Agent Resource Limits

Match agent resources to your GPU:

```json
{
  "preset": "small",      // 2GB RAM for 8B Q4 model
  "cpu_cores": 2,         // 2 CPU cores
  "env_vars": {
    "OPENAI_BASE_URL": "http://host.docker.internal:8081/v1"
  }
}
```

## Benchmarking

### Test Inference Speed

```bash
curl -X POST http://127.0.0.1:8081/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4o",
    "messages": [{"role": "user", "content": "Hello"}],
    "max_tokens": 100
  }'
```

Expected results for RTX 3070 + 8B Q4:
- Prompt: ~50-60 tokens/sec
- Generation: ~30-40 tokens/sec

## Troubleshooting Performance

### Slow Generation

**Symptoms:** < 20 tokens/sec generation

**Solutions:**
1. Check GPU utilization with `nvidia-smi`
2. Verify `--n-gpu-layers 99` is set
3. Reduce `--ctx-size` if running out of VRAM

### High Memory Usage

**Symptoms:** System swapping, slow overall

**Solutions:**
1. Reduce number of concurrent agents
2. Use smaller batch size
3. Close other applications

### Intermittent Slowdowns

**Symptoms:** Fast sometimes, slow other times

**Solutions:**
1. Check for thermal throttling
2. Disable Windows background services
3. Use consistent power plan (High Performance)

## Advanced: Multiple GPUs

If you have multiple GPUs:

```bash
# Use specific GPU (CUDA_VISIBLE_DEVICES)
CUDA_VISIBLE_DEVICES=0 ./llama-server.exe ...

# Or split model across GPUs (experimental)
--split-mode layer --split-mode-layers 4
```
