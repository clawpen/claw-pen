#!/usr/bin/env python3
"""Test connecting to Marketing agent through orchestrator"""

import asyncio
import websockets
import json
import urllib.request

async def test_marketing_through_orchestrator():
    # Get auth token
    try:
        login_data = json.dumps({"username": "admin", "password": "admin123"}).encode('utf-8')
        req = urllib.request.Request("http://127.0.0.1:8081/auth/login", data=login_data)
        req.add_header('Content-Type', 'application/json')
        with urllib.request.urlopen(req) as response:
            auth_data = json.loads(response.read())
            token = auth_data.get('token', '')
            print(f"[AUTH] Got token: {token[:20]}...")
    except Exception as e:
        print(f"[ERROR] Could not get token: {e}")
        return

    # Get Marketing agent ID
    with urllib.request.urlopen("http://127.0.0.1:8081/api/agents") as response:
        agents = json.loads(response.read())
        marketing = next((a for a in agents if a['name'] == 'Marketing'), None)
        if not marketing:
            print("[ERROR] Marketing agent not found")
            return
        agent_id = marketing['id']
        print(f"[AGENT] Found Marketing: {agent_id[:20]}...")

    # Connect through orchestrator
    ws_url = f"ws://127.0.0.1:8081/api/agents/{agent_id}/chat?token={token}"
    print(f"[WS] Connecting to: {ws_url[:80]}...")

    try:
        async with websockets.connect(ws_url, close_timeout=10) as ws:
            print("[OK] Connected to orchestrator!")

            # Wait for first message
            msg1 = json.loads(await ws.recv())
            print(f"[RECV 1] Type: {msg1.get('type')}, Event: {msg1.get('event')}")

            # Send connect message
            await ws.send(json.dumps({
                "type": "connect",
                "payload": {"userAgent": "test-client/1.0"}
            }))
            print("[SENT] Connect message")

            # Wait for more messages
            msg2 = json.loads(await ws.recv())
            print(f"[RECV 2] Type: {msg2.get('type')}, Event: {msg2.get('event')}, OK: {msg2.get('ok')}")

            print("[SUCCESS] Connection established!")

    except Exception as e:
        print(f"[ERROR] Connection failed: {e}")
        import traceback
        traceback.print_exc()

if __name__ == "__main__":
    asyncio.run(test_marketing_through_orchestrator())
