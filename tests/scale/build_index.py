#!/usr/bin/env python3
"""
Trigger IVF-PQ index build via the server's rebuild endpoint.
Or just wait for the background build to complete.

For now, this script just monitors the server logs for the
"IVF-PQ vector index built successfully" message.
"""
import sys
import time
import requests

SERVER = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8090"

print("Waiting for server to be ready...")
for i in range(60):
    try:
        r = requests.get(f"{SERVER}/v1/status", timeout=5)
        if r.ok:
            print(f"Server ready: {r.json().get('indexed_chunks', '?')} chunks")
            break
    except:
        pass
    time.sleep(2)
else:
    print("Server not ready after 2 minutes")
    sys.exit(1)

print("Server is up. IVF-PQ index builds in the background.")
print("Monitor server logs for 'IVF-PQ vector index built successfully'")
print("Once built, run: python3 tests/scale/query_bench.py")
