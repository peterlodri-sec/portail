"""
portail-mcp — MCP Gateway sidecar for Portail.

Wraps LiteLLM's MCP server manager as a Unix-socket ASGI server.
The Rust front-end (portail-proxy) proxies /mcp/* and /mcp-rest/* requests
to this process over a Unix domain socket.
"""
