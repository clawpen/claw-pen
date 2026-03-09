#!/bin/bash
# Claw Pen Electron Launcher with GPU workarounds

cd /data/claw-pen/electron-app

export ELECTRON_DISABLE_GPU=1
export LIBGL_ALWAYS_SOFTWARE=1

exec npx electron . \
    --disable-gpu \
    --disable-gpu-compositing \
    --disable-software-rasterizer \
    --disable-dev-shm-usage \
    --disable-accelerated-2d-canvas \
    --disable-accelerated-video-decode \
    --disable-accelerated-mjpeg-decode \
    --in-process-gpu \
    --no-sandbox \
    "$@"
