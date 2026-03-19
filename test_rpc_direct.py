#!/usr/bin/env python3
"""Direct test of agent-to-agent communication via Tailscale IPs"""

import asyncio
import websockets
import json
from datetime import datetime

# Agent configurations (from Docker logs)
AGENTS = {
    "RPCTest-1": {
        "tailscale_ip": "100.107.151.102",
        "gateway_port": 18803,
        "container_port": 18803
    },
    "RPCTest-2": {
        "tailscale_ip": "100.84.159.113",
        "gateway_port": 18804,
        "container_port": 18804
    },
    "TailscaleTest": {
        "tailscale_ip": "100.107.116.8",
        "gateway_port": 18801,
        "container_port": 18801
    }
}

async def test_direct_agent_communication():
    """Test direct WebSocket connection to agent gateways"""

    print("=" * 60)
    print("Testing Direct Agent-to-Agent Communication")
    print("=" * 60)
    print()

    # Test: Connect to RPCTest-1's gateway
    agent1_name = "RPCTest-1"
    agent1 = AGENTS[agent1_name]

    print(f"[Test 1] Connecting to {agent1_name} gateway...")
    print(f"  Tailscale IP: {agent1['tailscale_ip']}")
    print(f"  Gateway Port: {agent1['gateway_port']}")
    print()

    ws_url = f"ws://{agent1['tailscale_ip']}:{agent1['gateway_port']}/gateway"
    print(f"  WebSocket URL: {ws_url}")

    try:
        async with websockets.connect(ws_url) as websocket:
            print(f"  SUCCESS: Connected to {agent1_name} gateway!")
            print()

            # Send a test message
            test_message = {
                "type": "test",
                "timestamp": datetime.utcnow().isoformat(),
                "content": "Hello from RPC test!"
            }

            print(f"[Test 2] Sending test message...")
            await websocket.send(json.dumps(test_message))
            print(f"  Sent: {test_message}")
            print()

            # Wait for response
            print(f"[Test 3] Waiting for response...")
            try:
                response = await asyncio.wait_for(websocket.recv(), timeout=5)
                response_data = json.loads(response)
                print(f"  SUCCESS: Received response!")
                print(f"  Response: {json.dumps(response_data, indent=2)}")
            except asyncio.TimeoutError:
                print(f"  Note: No response (agent may not echo messages)")

    except Exception as e:
        print(f"  ERROR: {e}")
        print(f"  This is expected if:")
        print(f"    - Agent is not reachable via Tailscale")
        print(f"    - Gateway is not listening on the Tailscale interface")
        print(f"    - Firewall is blocking the connection")

    print()
    print("=" * 60)
    print("Test Complete")
    print("=" * 60)

if __name__ == "__main__":
    asyncio.run(test_direct_agent_communication())
