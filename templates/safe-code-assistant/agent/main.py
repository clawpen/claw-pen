"""
Safe Code Assistant - Network-isolated agent for code analysis
"""
import os
import sys
from pathlib import Path

class SafeCodeAssistant:
    """
    A code assistant that:
    - Has NO network access (verified at startup)
    - Can only read mounted directories
    - Uses local LLM via mounted socket
    """
    
    def __init__(self):
        self._verify_security()
        self.workspace = Path(os.environ.get("WORKSPACE", "/workspace"))
        
    def _verify_security(self):
        """Verify we're running in secure mode"""
        # Check network is disabled
        try:
            import socket
            socket.create_connection(("8.8.8.8", 53), timeout=1)
            raise RuntimeError("SECURITY: Network access detected! This agent should be network-isolated.")
        except (socket.timeout, OSError):
            pass  # Good - no network
        
        # Verify no secrets in environment
        dangerous_vars = ["AWS_", "GITHUB_TOKEN", "API_KEY"]
        for var in os.environ:
            for danger in dangerous_vars:
                if danger in var.upper():
                    print(f"Warning: {var} is set (should use secrets injection)")
    
    def analyze_file(self, path: str) -> dict:
        """Analyze a single file"""
        full_path = self.workspace / path
        if not full_path.exists():
            return {"error": "File not found"}
        
        # Security: ensure path is within workspace
        try:
            full_path.resolve().relative_to(self.workspace.resolve())
        except ValueError:
            return {"error": "Access denied - path outside workspace"}
        
        return {
            "path": str(path),
            "size": full_path.stat().st_size,
            "content": full_path.read_text()[:10000],  # First 10KB
        }
    
    def search(self, query: str, limit: int = 10) -> list:
        """Semantic search in indexed code"""
        # TODO: Use code_index module
        return []

if __name__ == "__main__":
    agent = SafeCodeAssistant()
    print("Safe Code Assistant initialized")
    print(f"Workspace: {agent.workspace}")
