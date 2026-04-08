#!/usr/bin/env python3
"""Test agent response to debug empty content issue"""

import asyncio
import websockets
import json
import uuid

async def test_agent():
    uri = "ws://127.0.0.1:18790/chat"

    try:
        async with websockets.connect(uri) as ws:
            print("[OK] Connected to agent")

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
                print(f"[RECV] {json.dumps(msg, indent=2)[:200]}")

                if msg.get("type") == "res" and msg.get("ok"):
                    print("\n[OK] Authenticated!\n")
                    break

            # Send chat message
            chat_req = {
                "type": "req",
                "id": str(uuid.uuid4()),
                "method": "chat.send",
                "params": {
                    "sessionKey": "agent:dev:main",
                    "message": "Say hello in one sentence",
                    "idempotencyKey": f"idem-{asyncio.get_event_loop().time()}"
                }
            }
            print(f"[SEND] Chat message: {chat_req['params']['message']}")
            await ws.send(json.dumps(chat_req))

            # Collect all responses
            print("\n[WAIT] for responses...\n")
            timeout_count = 0
            max_timeouts = 30  # 30 seconds max

            while timeout_count < max_timeouts:
                try:
                    response = await asyncio.wait_for(ws.recv(), timeout=1.0)
                    msg = json.loads(response)
                    print(f"[RECV] Type: {msg.get('type')}, Event: {msg.get('event')}")

                    # Print full message for debugging
                    if msg.get("type") == "event":
                        payload = msg.get("payload", {})
                        print(f"  Payload: {json.dumps(payload, indent=2)[:300]}")

                        # Check for agent events with stream data
                        if msg.get("event") == "agent":
                            stream = payload.get("stream")
                            data = payload.get("data", {})
                            print(f"  Stream: {stream}")
                            print(f"  Data: {json.dumps(data, indent=2)[:300]}")

                            # Check if this is the end event
                            if stream == "lifecycle" and data.get("phase") == "end":
                                print("\n[DONE] Agent finished")
                                break

                except asyncio.TimeoutError:
                    timeout_count += 1
                    print(f"[TICK] {timeout_count}s waiting...")

            print("\n[Test complete]")

    except Exception as e:
        print(f"[ERROR] {e}")
        import traceback
        traceback.print_exc()

if __name__ == "__main__":
    asyncio.run(test_agent())
