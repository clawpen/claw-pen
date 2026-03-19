#!/usr/bin/env python3
"""Test RPC communication between agents"""

import requests
import json
import asyncio
import websockets
from datetime import datetime

ORCHESTRATOR_URL = "http://localhost:8081"
AUTH_TOKEN = "admin-token"  # Default admin token

def get_agents():
    """Get list of all agents"""
    response = requests.get(
        f"{ORCHESTRATOR_URL}/api/agents",
        headers={"Authorization": f"Bearer {AUTH_TOKEN}"}
    )
    return response.json()

def get_running_agents_with_tailscale_ips():
    """Get running agents that have Tailscale IPs"""
    agents = get_agents()
    return [
        agent for agent in agents
        if agent["status"] == "running" and agent.get("tailscale_ip")
    ]

async def test_agent_communication():
    """Test communication between two agents via WebSocket"""

    # Get running agents with Tailscale IPs
    agents = get_running_agents_with_tailscale_ips()

    if len(agents) < 2:
        print(f"Need at least 2 agents with Tailscale IPs")
        print(f"   Found: {len(agents)}")
        for agent in agents:
            print(f"   - {agent['name']}: {agent.get('tailscale_ip', 'no IP')}")
        return

    agent1 = agents[0]
    agent2 = agents[1]

    print(f"Found 2 agents with Tailscale IPs:")
    print(f"   1. {agent1['name']}: {agent1['tailscale_ip']} (port {agent1['gateway_port']})")
    print(f"   2. {agent2['name']}: {agent2['tailscale_ip']} (port {agent2['gateway_port']})")
    print()

    # Test: Send message from agent1 to agent2
    print(f"Testing message from {agent1['name']} to {agent2['name']}...")

    # Create a test message
    test_message = {
        "type": "test",
        "from": agent1['id'],
        "to": agent2['id'],
        "content": "Hello from RPC test!",
        "timestamp": datetime.utcnow().isoformat()
    }

    try:
        # Connect to agent1's WebSocket
        ws_url = f"ws://localhost:8081/api/agents/{agent1['id']}/chat?token={AUTH_TOKEN}"

        async with websockets.connect(ws_url) as websocket:
            print(f"Connected to {agent1['name']}")

            # Send the message
            await websocket.send(json.dumps({
                "type": "agent_message",
                "target_agent_id": agent2['id'],
                "message": test_message
            }))

            print(f"Sent message to agent {agent2['name']}")

            # Wait for response
            response = await asyncio.wait_for(websocket.recv(), timeout=10)
            response_data = json.loads(response)

            print(f"Received response: {response_data.get('type', 'unknown')}")

    except asyncio.TimeoutError:
        print(f"Timeout waiting for response")
    except Exception as e:
        print(f"Error: {e}")

def main():
    print("Testing Agent-to-Agent RPC Communication")
    print("=" * 50)
    print()

    # Check agents
    agents = get_agents()
    running_agents = [a for a in agents if a["status"] == "running"]

    print(f"Agent Status:")
    print(f"   Total agents: {len(agents)}")
    print(f"   Running: {len(running_agents)}")

    print(f"\nRunning Agents:")
    for agent in running_agents:
        ip = agent.get("tailscale_ip", "no IP")
        print(f"   - {agent['name']}: {ip} (port {agent['gateway_port']})")

    print()

    # Test communication
    asyncio.run(test_agent_communication())

if __name__ == "__main__":
    main()
