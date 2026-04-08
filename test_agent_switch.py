#!/usr/bin/env python3
"""Test switching between agents through orchestrator"""

import asyncio
import websockets
import json
import uuid

async def test_agent_switch():
    # Get the agent IDs from the orchestrator
    import urllib.request
    agents_url = "http://127.0.0.1:8081/api/agents"

    try:
        with urllib.request.urlopen(agents_url) as response:
            agents = json.loads(response.read())
            print(f"[AGENTS] Found {len(agents)} agents:")
            for agent in agents:
                print(f"  - {agent['name']}: {agent['id'][:16]}... (port {agent['gateway_port']})")

            if len(agents) < 2:
                print("\n[ERROR] Need at least 2 agents to test switching")
                return

            agent1 = agents[0]
            agent2 = agents[1]

            print(f"\n[TEST] Will switch from {agent1['name']} to {agent2['name']}")

            # Get auth token
            login_url = "http://127.0.0.1:8081/auth/login"
            login_data = json.dumps({"username": "admin", "password": "admin123"}).encode('utf-8')

            try:
                with urllib.request.urlopen(login_url, data=login_data) as response:
                    auth_data = json.loads(response.read())
                    token = auth_data.get('token', '')
                    print(f"[AUTH] Got token: {token[:20]}...")
            except:
                print("[WARN] Could not get token, testing without auth")
                token = ''

            # Test connecting to agent 1
            print(f"\n[STEP 1] Connecting to {agent1['name']}...")
            ws_url1 = f"ws://127.0.0.1:8081/api/agents/{agent1['id']}/chat?token={token}"
            print(f"[URL] {ws_url1}")

            try:
                async with websockets.connect(ws_url1, close_timeout=10) as ws1:
                    print("[OK] Connected to agent 1!")

                    # Wait for challenge and auth
                    msg1 = json.loads(await ws1.recv())
                    print(f"[RECV] {msg1.get('event')}")

                    connect_req = {
                        "type": "req",
                        "id": str(uuid.uuid4()),
                        "method": "connect",
                        "params": {
                            "minProtocol": 3,
                            "maxProtocol": 3,
                            "client": {"id": "cli", "version": "1.0.0", "platform": "python", "mode": "cli"},
                            "role": "operator",
                            "scopes": ["operator.read", "operator.write"],
                            "auth": {"password": "clawpen"}
                        }
                    }
                    await ws1.send(json.dumps(connect_req))

                    # Wait for auth response
                    msg2 = json.loads(await ws1.recv())
                    if msg2.get("ok"):
                        print("[OK] Authenticated with agent 1!")
                    else:
                        print(f"[ERROR] Auth failed: {msg2}")
                        return

            except Exception as e:
                print(f"[ERROR] Failed to connect to agent 1: {e}")
                return

            # Wait a bit
            await asyncio.sleep(1)

            # Now test connecting to agent 2 (simulating switch)
            print(f"\n[STEP 2] Switching to {agent2['name']}...")
            ws_url2 = f"ws://127.0.0.1:8081/api/agents/{agent2['id']}/chat?token={token}"
            print(f"[URL] {ws_url2}")

            try:
                async with websockets.connect(ws_url2, close_timeout=10) as ws2:
                    print("[OK] Connected to agent 2!")

                    # Wait for challenge and auth
                    msg1 = json.loads(await ws2.recv())
                    print(f"[RECV] {msg1.get('event')}")

                    await ws2.send(json.dumps(connect_req))

                    # Wait for auth response
                    msg2 = json.loads(await ws2.recv())
                    if msg2.get("ok"):
                        print("[OK] Authenticated with agent 2!")
                    else:
                        print(f"[ERROR] Auth failed: {msg2}")
                        return

            except Exception as e:
                print(f"[ERROR] Failed to connect to agent 2: {e}")
                import traceback
                traceback.print_exc()
                return

            print("\n[SUCCESS] Agent switching works!")

    except Exception as e:
        print(f"[ERROR] {e}")
        import traceback
        traceback.print_exc()

if __name__ == "__main__":
    asyncio.run(test_agent_switch())
