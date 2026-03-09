#!/bin/bash
# Start Claw Pen Electron app in background

cd /data/claw-pen/electron-app

export ELECTRON_DISABLE_GPU=1
export LIBGL_ALWAYS_SOFTWARE=1

nohup npx electron . \
    --disable-gpu \
    --no-sandbox \
    --disable-gpu-compositing \
    --in-process-gpu \
    > /tmp/claw-pen-electron.log 2>&1 &

echo "Started with PID: $!"
echo "Log file: /tmp/claw-pen-electron.log"
sleep 3
echo ""
echo "Initial log output:"
head -30 /tmp/claw-pen-electron.log
