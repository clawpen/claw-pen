#!/bin/bash
# Start Claw Pen Orchestrator

cd /root/.openclaw/workspace/claw-pen

export RUST_LOG=info
export HOST="::"
export PORT=3001

exec ./orchestrator-bin -c claw-pen.toml 2>&1
