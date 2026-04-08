#!/usr/bin/env python3
# -*- coding: utf-8 -*-
import asyncio
import websockets
import json
import sys
import io

# Fix Windows console encoding
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')

async def test_agent():
    uri = "ws://127.0.0.1:18790/chat"

    try:
        async with websockets.connect(uri) as websocket:
            print("[OK] Connected to agent!")

            # Wait for challenge
            message = await websocket.recv()
            data = json.loads(message)
            print(f"[RECV] Received: {json.dumps(data, indent=2)}")

            if data.get("type") == "event" and data.get("event") == "connect.challenge":
                nonce = data["payload"]["nonce"]
                print(f"[AUTH] Challenge received, nonce: {nonce}")

                # Respond with password
                auth_response = {
                    "type": "action",
                    "action": "connect.authenticate",
                    "payload": {
                        "password": "clawpen",
                        "nonce": nonce
                    }
                }
                await websocket.send(json.dumps(auth_response))
                print(f"[SEND] Sent authentication response: {json.dumps(auth_response, indent=2)}")

            # Wait for response
            try:
                response = await asyncio.wait_for(websocket.recv(), timeout=5.0)
                print(f"[RECV] Received after auth: {response}")
            except asyncio.TimeoutError:
                print("[TIMEOUT] Timeout waiting for authentication response")

            # Check if still connected
            if websocket.open:
                print("[OK] Still connected after authentication!")

                # Send a test message
                test_msg = {
                    "type": "action",
                    "action": "chat",
                    "payload": {
                        "content": "Hello! Please respond with just the word SUCCESS"
                    }
                }
                await websocket.send(json.dumps(test_msg))
                print(f"[SEND] Sent test message")

                # Wait for response
                while True:
                    try:
                        response = await asyncio.wait_for(websocket.recv(), timeout=10.0)
                        print(f"[RECV] Response: {response}")
                    except asyncio.TimeoutError:
                        print("[TIMEOUT] No more responses (timeout)")
                        break
            else:
                print("[ERROR] Connection closed after authentication attempt")

    except websockets.exceptions.ConnectionClosed as e:
        print(f"[ERROR] Connection closed: code={e.code}, reason={e.reason}")
    except Exception as e:
        print(f"[ERROR] Error: {type(e).__name__}: {e}")

if __name__ == "__main__":
    asyncio.run(test_agent())
