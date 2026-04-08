#!/bin/bash
# Build llama.cpp CUDA image for Claw Pen local inference

IMAGE_NAME="clawpen-local-inference"
IMAGE_TAG="cuda-latest"

echo "Building llama.cpp with CUDA support..."
echo "This may take 10-20 minutes..."

docker build -f orchestrator/Dockerfile.llamacpp-cuda \
    -t ${IMAGE_NAME}:${IMAGE_TAG} \
    -t ${IMAGE_NAME}:latest \
    --progress=plain \
    .

echo ""
echo "Build complete!"
echo "Image: ${IMAGE_NAME}:${IMAGE_TAG}"
echo ""
echo "To run local inference:"
echo "  docker run -d --gpus all -p 8080:8080 -v /path/to/models:/app/models ${IMAGE_NAME}:latest"
echo ""
echo "Available models:"
echo "  - Qwen2.5-7B-Instruct-Q4_K_M.gguf (~5GB)"
echo "  - Llama-3.2-3B-Instruct-Q4_K_M.gguf (~2GB)"
echo "  - Phi-3-mini-4k-instruct-q4.gguf (~2.5GB)"
