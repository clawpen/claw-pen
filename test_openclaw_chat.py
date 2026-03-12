#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Direct test: Client → OpenClaw Agent (bypassing orchestrator for now)
This tests the complete message format we'll use in orchestrator
"""
import asyncio
import websockets
import json
import sys
import io
import uuid

# Fix Windows console encoding
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')

async def test_openclaw_chat():
    """
    Test the exact message format the orchestrator will use
    """
    uri = "ws://127.0.0.1:18790/chat"  # Port 18790 (orchestrator default)

    try:
        async with websockets.connect(uri) as ws:
            print("[OK] Connected to OpenClaw agent!")

            # Wait for challenge
            message = await ws.recv()
            data = json.loads(message)
            print(f"[RECV] Challenge: {json.dumps(data, indent=2)}")

            # Send CONNECT request (orchestrator format)
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

            await ws.send(json.dumps(connect_request))
            print(f"[SEND] Connect request sent")

            # Wait for response
            response = await ws.recv()
            response_data = json.loads(response)
            print(f"[RECV] Connect response: ok={response_data.get('ok')}")

            if not response_data.get("ok"):
                print(f"[ERROR] Authentication failed: {response}")
                return

            print("\n✅ Authentication successful!\n")

            # Now send a chat message using the orchestrator's translation format
            print("[SEND] Testing chat.send message format...")

            # This is what the orchestrator will send
            chat_request = {
                "type": "req",
                "id": f"req-{uuid.uuid4()}",
                "method": "chat.send",
                "params": {
                    "sessionKey": "agent:dev:main",
                    "message": "Hello! Please respond with just the word SUCCESS",
                    "idempotencyKey": f"idem-{uuid.uuid4()}"
                }
            }

            await ws.send(json.dumps(chat_request))
            print(f"[SEND] Chat message sent")

            # Wait for responses
            print("\n[WAIT] For agent response...\n")

            timeout_count = 0
            max_timeout = 30

            while timeout_count < max_timeout:
                try:
                    response = await asyncio.wait_for(ws.recv(), timeout=1.0)
                    response_data = json.loads(response)

                    # Pretty print based on message type
                    msg_type = response_data.get("type", "")
                    if msg_type == "event":
                        event = response_data.get("event", "")
                        payload = response_data.get("payload", {})

                        if event == "chat.chunk":
                            # Chat response chunks
                            content = payload.get("content", "")
                            print(content, end="", flush=True)
                            if "SUCCESS" in content:
                                print("\n\n✅ SUCCESS! Agent responded correctly!")
                        elif event == "chat.done":
                            # Chat completed
                            reason = payload.get("reason", "unknown")
                            print(f"\n\n[DONE] Chat completed: {reason}")
                        elif event == "agent":
                            # Agent lifecycle/progress events
                            data = payload.get("data", {})
                            stream = payload.get("stream", "")
                            if stream == "assistant":
                                # Assistant response text
                                text = data.get("text", "")
                                print(text, end="", flush=True)
                            elif stream == "lifecycle":
                                phase = data.get("phase", "")
                                print(f"\n[AGENT] {phase}...")
                        elif event == "health":
                            # Health check - skip
                            pass
                        elif event == "tick":
                            # Tick event - skip
                            pass
                        else:
                            print(f"\n[EVENT {event}]")
                    elif msg_type == "res":
                        ok = response_data.get("ok", False)
                        if not ok:
                            error = response_data.get("error", {})
                            print(f"\n[ERROR] {error}")
                        else:
                            print(f"\n[ACK] Request acknowledged")
                    else:
                        print(f"\n[RECV] {response[:100]}")

                except asyncio.TimeoutError:
                    timeout_count += 1

            if timeout_count >= max_timeout:
                print("\n⚠️  Timeout - agent may take longer to respond")

    except websockets.exceptions.ConnectionClosed as e:
        print(f"\n[ERROR] Connection closed: code={e.code}, reason={e.reason}")
    except Exception as e:
        print(f"\n[ERROR] {type(e).__name__}: {e}")

if __name__ == "__main__":
    print("=" * 70)
    print("Testing OpenClaw Chat Protocol (Orchestrator Message Format)")
    print("=" * 70)
    asyncio.run(test_openclaw_chat())
    print("\n" + "=" * 70)
