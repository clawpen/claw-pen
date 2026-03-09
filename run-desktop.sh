#!/bin/bash
# Claw Pen Desktop Launcher
# Workaround for WebKit2GTK stability on Linux

export LIBGL_ALWAYS_SOFTWARE=1
export WEBKIT_DISABLE_COMPOSITING_MODE=1

exec /data/claw-pen/target/release/claw-pen-desktop "$@"
