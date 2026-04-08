#!/usr/bin/env python3
"""Test RevitClaw agent connection"""

import asyncio
import websockets
import json
import uuid

async def test_agent():
    uri = "ws://127.0.0.1:18791/chat"  # RevitClaw port

    try:
        print("[CONNECT] Attempting to connect to RevitClaw...")
        async with websockets.connect(uri, close_timeout=10) as ws:
            print("[OK] Connected to RevitClaw")

            # Wait for challenge
            challenge = json.loads(await ws.recv())
            print(f"[RECV] Challenge: {challenge.get('event')}")

            # Send connect request
            connect_req = {
                "type": "req",
                "id": str(uuid.uuid4()),
                "method": "connect",
                "params": {
                    "minProtocol": 3,
                    "maxProtocol": 3,
                    "client": {
                        "id": "cli",
                        "version": "1.0.0",
                        "platform": "python",
                        "mode": "cli"
                    },
                    "role": "operator",
                    "scopes": ["operator.read", "operator.write"],
                    "auth": {
                        "password": "clawpen"
                    }
                }
            }
            await ws.send(json.dumps(connect_req))
            print("[SEND] Connect request")

            # Wait for connect response
            while True:
                msg = json.loads(await ws.recv())
                print(f"[RECV] Type: {msg.get('type')}, OK: {msg.get('ok')}")

                if msg.get("type") == "res" and msg.get("ok"):
                    print("\n[OK] Authenticated!\n")
                    break

            print("[SUCCESS] RevitClaw is working!")
            return

    except websockets.exceptions.InvalidStatusCode as e:
        print(f"[ERROR] Invalid status code: {e.status_code}")
        print(f"[ERROR] This usually means the websocket endpoint is wrong")
    except websockets.exceptions.ConnectionClosed as e:
        print(f"[ERROR] Connection closed: {e}")
    except Exception as e:
        print(f"[ERROR] {e}")
        import traceback
        traceback.print_exc()

if __name__ == "__main__":
    asyncio.run(test_agent())
