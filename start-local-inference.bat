@echo off
REM Start llama.cpp server for Claw Pen local inference

cd "F:\Software\Claw Pen\llamacpp"

start "llama.cpp Server" /MIN llama-server.exe ^
  --model "F:\Software\Claw Pen\claw-pen\data\models\Llama-3.3-8B-Instruct-Thinking-Claude-Haiku-4.5-High-Reasoning-1700x.Q4_K_M.gguf" ^
  --host 0.0.0.0 ^
  --port 8081 ^
  --ctx-size 8192 ^
  --n-gpu-layers 99 ^
  --n-parallel 4 ^
  --metrics

echo Waiting for server to start...
timeout /t 30 /nobreak

echo Testing server...
curl -s http://localhost:8081/health

echo.
echo Server should be running on http://localhost:8081
echo Press Ctrl+C to stop the server window
pause
