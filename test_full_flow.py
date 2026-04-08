#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Test the full flow: Client → Orchestrator → OpenClaw Agent → Zai GLM-5
"""
import asyncio
import websockets
import json
import sys
import io

# Fix Windows console encoding
sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')

async def test_full_flow():
    """
    Connect to orchestrator and send a message through to OpenClaw agent
    """
    # First, get auth token from orchestrator
    import aiohttp

    print("[1] Getting auth token from orchestrator...")
    async with aiohttp.ClientSession() as session:
        # Login to get token
        login_payload = {
            "username": "admin",
            "password": "admin123"  # Default password
        }

        try:
            async with session.post("http://127.0.0.1:8081/auth/login",
                                   json=login_payload) as resp:
                if resp.status == 200:
                    data = await resp.json()
                    token = data.get("token")
                    print(f"[OK] Got auth token: {token[:20]}...")
                else:
                    text = await resp.text()
                    print(f"[ERROR] Login failed: {resp.status} - {text}")
                    return
        except Exception as e:
            print(f"[ERROR] Failed to connect to orchestrator: {e}")
            return

    # Now connect via WebSocket
    ws_url = "ws://127.0.0.1:8081/agents/openclaw-agent/chat"
    headers = {"Authorization": f"Bearer {token}"}

    print(f"[2] Connecting to orchestrator at {ws_url}...")

    try:
        async with websockets.connect(ws_url, extra_headers=headers) as ws:
            print("[OK] Connected to orchestrator!")

            # Send a test message
            test_message = {
                "content": "Hello! Please respond with just the word SUCCESS",
                "session": "main"
            }

            print(f"[3] Sending message: {test_message['content']}")
            await ws.send(json.dumps(test_message))

            # Wait for response
            print("[4] Waiting for response from agent...")

            timeout_count = 0
            max_timeout = 30  # 30 seconds

            while timeout_count < max_timeout:
                try:
                    response = await asyncio.wait_for(ws.recv(), timeout=1.0)
                    print(f"[RECV] {response[:200]}..." if len(response) > 200 else f"[RECV] {response}")

                    # Check if this is the agent's response
                    if "SUCCESS" in response:
                        print("\n✅ SUCCESS! Full flow is working!")
                        print("   GUI → Orchestrator → OpenClaw Agent → Zai GLM-5")
                        break

                except asyncio.TimeoutError:
                    timeout_count += 1
                    if timeout_count % 5 == 0:
                        print(f"   Waiting... ({timeout_count}s)")

            if timeout_count >= max_timeout:
                print("\n⚠️  Timeout waiting for response")
                print("   The agent might be processing a long request")

    except websockets.exceptions.ConnectionClosed as e:
        print(f"[ERROR] Connection closed: code={e.code}, reason={e.reason}")
    except Exception as e:
        print(f"[ERROR] {type(e).__name__}: {e}")

if __name__ == "__main__":
    print("=" * 60)
    print("Testing Full Flow: GUI → Orchestrator → OpenClaw → Zai")
    print("=" * 60)
    asyncio.run(test_full_flow())
    print("=" * 60)
