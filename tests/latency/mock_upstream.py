#!/usr/bin/env python3
"""
Mock Anthropic upstream server for latency benchmarking.

Returns canned responses with configurable delay to simulate LLM think time.
When the request contains memoryport_* tools, returns a tool_use response on
the first call (per conversation), then a final text response on the follow-up.

Usage:
    python3 tests/latency/mock_upstream.py [--port 8199] [--delay-ms 50]
"""

import argparse
import json
import time
import uuid
from http.server import HTTPServer, BaseHTTPRequestHandler
from threading import Lock

# Track conversations that have already done their tool round.
# Key = frozenset of first user message content, Value = True
_completed_tool_rounds: dict[str, bool] = {}
_lock = Lock()


def canned_text_response(model: str = "mock-model") -> dict:
    """A simple Anthropic-format text response."""
    return {
        "id": f"msg_{uuid.uuid4().hex[:16]}",
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": [
            {
                "type": "text",
                "text": "This is a mock response for latency benchmarking.",
            }
        ],
        "stop_reason": "end_turn",
        "usage": {"input_tokens": 50, "output_tokens": 20},
    }


def canned_tool_use_response(model: str = "mock-model") -> dict:
    """An Anthropic-format response that calls memoryport_search."""
    return {
        "id": f"msg_{uuid.uuid4().hex[:16]}",
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": [
            {
                "type": "text",
                "text": "Let me search your memory for relevant context.",
            },
            {
                "type": "tool_use",
                "id": f"toolu_{uuid.uuid4().hex[:16]}",
                "name": "memoryport_search",
                "input": {"query": "benchmark test query", "max_results": 5},
            },
        ],
        "stop_reason": "tool_use",
        "usage": {"input_tokens": 80, "output_tokens": 40},
    }


class MockHandler(BaseHTTPRequestHandler):
    delay_ms: float = 50

    def log_message(self, format, *args):
        pass  # suppress request logs

    def do_POST(self):
        content_length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(content_length)

        # Simulate LLM latency
        time.sleep(self.delay_ms / 1000.0)

        try:
            request = json.loads(body) if body else {}
        except json.JSONDecodeError:
            request = {}

        model = request.get("model", "mock-model")
        has_memoryport_tools = False
        conversation_key = ""

        # Check if request has memoryport tools injected
        tools = request.get("tools", [])
        for tool in tools:
            name = tool.get("name", "")
            if name.startswith("memoryport_"):
                has_memoryport_tools = True
                break

        # Extract conversation key from first user message
        messages = request.get("messages", [])
        for msg in messages:
            if msg.get("role") == "user":
                content = msg.get("content", "")
                if isinstance(content, str):
                    conversation_key = content[:100]
                elif isinstance(content, list):
                    # Check if it's a tool_result (follow-up in agentic loop)
                    for block in content:
                        if isinstance(block, dict) and block.get("type") == "tool_result":
                            conversation_key = "__tool_followup__"
                            break
                        elif isinstance(block, dict) and block.get("type") == "text":
                            conversation_key = block.get("text", "")[:100]
                            break
                break

        # Decide response: tool_use or text
        if has_memoryport_tools and conversation_key != "__tool_followup__":
            # First call with tools — return tool_use to trigger agentic round
            with _lock:
                already_done = _completed_tool_rounds.get(conversation_key, False)
                if not already_done:
                    _completed_tool_rounds[conversation_key] = True
                    response = canned_tool_use_response(model)
                else:
                    response = canned_text_response(model)
        else:
            # No tools or follow-up — return plain text
            response = canned_text_response(model)

        response_bytes = json.dumps(response).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(response_bytes)))
        self.end_headers()
        self.wfile.write(response_bytes)

    def do_GET(self):
        # Health check
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(b'{"status":"ok"}')


def main():
    parser = argparse.ArgumentParser(description="Mock Anthropic upstream for latency benchmarking")
    parser.add_argument("--port", type=int, default=8199, help="Port to listen on (default: 8199)")
    parser.add_argument("--delay-ms", type=float, default=50, help="Simulated LLM latency in ms (default: 50)")
    args = parser.parse_args()

    MockHandler.delay_ms = args.delay_ms

    server = HTTPServer(("127.0.0.1", args.port), MockHandler)
    print(f"Mock upstream listening on http://127.0.0.1:{args.port} (delay={args.delay_ms}ms)")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down.")
        server.server_close()


if __name__ == "__main__":
    main()
