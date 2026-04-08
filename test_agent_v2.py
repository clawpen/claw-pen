#!/usr/bin/env python3
# -*- coding: utf-8 -*-
import asyncio
import websockets
import json
import sys
import io
import uuid

# Fix Windows console encoding
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')

async def test_agent():
    uri = "ws://127.0.0.1:18791/chat"  # OpenClaw agent container

    try:
        async with websockets.connect(uri) as websocket:
            print("[OK] Connected to agent!")

            # Wait for challenge
            message = await websocket.recv()
            data = json.loads(message)
            print(f"[RECV] Challenge: {json.dumps(data, indent=2)}")

            nonce = None
            if data.get("type") == "event" and data.get("event") == "connect.challenge":
                nonce = data["payload"]["nonce"]
                print(f"[AUTH] Challenge received, nonce: {nonce}")

            # Send CONNECT request with password auth (CORRECT FORMAT)
            connect_request = {
                "type": "req",
                "id": str(uuid.uuid4()),
                "method": "connect",
                "params": {
                    "minProtocol": 3,
                    "maxProtocol": 3,
                    "client": {
                        "id": "cli",
                        "version": "1.0.0",
                        "platform": "windows",
                        "mode": "cli"
                    },
                    "role": "operator",
                    "scopes": ["operator.read", "operator.write"],
                    "caps": [],
                    "commands": [],
                    "permissions": {},
                    "auth": {
                        "password": "clawpen"
                    },
                    "locale": "en-US",
                    "userAgent": "test-client/1.0.0"
                }
            }

            await websocket.send(json.dumps(connect_request))
            print(f"[SEND] Connect request: {json.dumps(connect_request, indent=2)}")

            # Wait for response
            try:
                response = await asyncio.wait_for(websocket.recv(), timeout=5.0)
                response_data = json.loads(response)
                print(f"[RECV] Response: {json.dumps(response_data, indent=2)}")

                if response_data.get("ok"):
                    print("[OK] Authentication successful!")

                    # Send a test message
                    test_msg = {
                        "type": "req",
                        "id": str(uuid.uuid4()),
                        "method": "chat.send",
                        "params": {
                            "content": "Hello! Please respond with just the word SUCCESS",
                            "session": "main"
                        }
                    }
                    await websocket.send(json.dumps(test_msg))
                    print(f"[SEND] Test message")

                    # Wait for response
                    while True:
                        try:
                            response = await asyncio.wait_for(websocket.recv(), timeout=10.0)
                            print(f"[RECV] Response: {response}")
                        except asyncio.TimeoutError:
                            print("[TIMEOUT] No more responses (timeout)")
                            break
                else:
                    print(f"[ERROR] Authentication failed: {response_data}")
            except asyncio.TimeoutError:
                print("[TIMEOUT] Timeout waiting for authentication response")

    except websockets.exceptions.ConnectionClosed as e:
        print(f"[ERROR] Connection closed: code={e.code}, reason={e.reason}")
    except Exception as e:
        print(f"[ERROR] Error: {type(e).__name__}: {e}")

if __name__ == "__main__":
    asyncio.run(test_agent())
