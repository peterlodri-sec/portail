"""
Portail MCP sidecar — ASGI server on a Unix socket.

Receives framed HTTP requests from the Rust front-end over a Unix domain
socket, routes them to the MCP gateway, and sends back framed responses.

Protocol:
  Request:  <method_len:2B><method><path_len:4B><path>
            <headers_len:4B><headers_json><body_len:8B><body>
  Response: <status:2B><headers_len:4B><headers_json>
            <body_len:8B><body>
"""

from __future__ import annotations

import argparse
import asyncio
import json
import logging
import os
import struct
import sys
from pathlib import Path

from .gateway import PortailMcpGateway

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
)
logger = logging.getLogger("portail.mcp.server")


class UnixSocketServer:
    """Minimal ASGI-like server over a Unix socket using framed messages."""

    def __init__(self, socket_path: str, gateway: PortailMcpGateway):
        self.socket_path = socket_path
        self.gateway = gateway

    async def serve(self) -> None:
        sock_path = Path(self.socket_path)

        # Ensure parent exists
        sock_path.parent.mkdir(parents=True, exist_ok=True)

        # Remove stale socket
        if sock_path.exists():
            sock_path.unlink()

        server = await asyncio.start_unix_server(
            self.handle_connection,
            path=str(sock_path),
        )

        logger.info("MCP sidecar listening on %s", self.socket_path)
        await self.gateway.initialise()

        async with server:
            await server.serve_forever()

    async def handle_connection(
        self,
        reader: asyncio.StreamReader,
        writer: asyncio.StreamWriter,
    ) -> None:
        try:
            # Read framed request
            request = await self._read_frame(reader)
            if request is None:
                return

            # Route
            response = await self._route(request)

            # Write framed response
            await self._write_frame(writer, response)
        except Exception as e:
            logger.error("connection error: %s", e)
            try:
                await self._write_frame(
                    writer,
                    {
                        "status": 500,
                        "headers": {"content-type": "application/json"},
                        "body": json.dumps({"error": str(e)}).encode(),
                    },
                )
            except Exception:
                pass
        finally:
            try:
                writer.close()
            except Exception:
                pass

    async def _route(self, request: dict) -> dict:
        method = request["method"]
        path = request["path"]
        headers = request["headers"]
        body = request["body"]

        # Parse body for POST requests
        body_json = {}
        if body and method == "POST":
            try:
                body_json = json.loads(body)
            except json.JSONDecodeError:
                body_json = {"raw": body.decode(errors="replace")}

        # Health
        if path in ("/healthz", "/livez", "/readyz"):
            health = await self.gateway.health()
            return {
                "status": 200,
                "headers": {"content-type": "application/json"},
                "body": json.dumps(health).encode(),
            }

        # MCP REST API: list tools
        if path == "/mcp-rest/tools/list" or path.endswith("/tools/list"):
            api_key = headers.get("authorization", "").removeprefix("Bearer ")
            tools = await self.gateway.list_tools(api_key=api_key or None)
            return {
                "status": 200,
                "headers": {"content-type": "application/json"},
                "body": json.dumps({"tools": tools}).encode(),
            }

        # MCP REST API: call tool
        if path == "/mcp-rest/tools/call" or path.endswith("/tools/call"):
            tool_name = body_json.get("name", body_json.get("tool_name", ""))
            arguments = body_json.get("arguments", body_json.get("input", {}))
            api_key = headers.get("authorization", "").removeprefix("Bearer ")
            result = await self.gateway.call_tool(
                tool_name=tool_name,
                arguments=arguments,
                api_key=api_key or None,
            )
            return {
                "status": 200,
                "headers": {"content-type": "application/json"},
                "body": json.dumps(result).encode(),
            }

        # MCP native protocol (SSE / Streamable HTTP)
        if path.startswith("/mcp/") or path == "/mcp":
            return await self._handle_mcp_protocol(method, path, headers, body_json)

        # Fallback
        health = await self.gateway.health()
        return {
            "status": 200,
            "headers": {"content-type": "application/json"},
            "body": json.dumps(health).encode(),
        }

    async def _handle_mcp_protocol(
        self,
        method: str,
        path: str,
        headers: dict,
        body: dict,
    ) -> dict:
        """Handle MCP native protocol messages."""
        # Streamable HTTP MCP — POST /mcp with JSON-RPC body
        if method == "POST" and body.get("method"):
            method_name = body.get("method")
            params = body.get("params", {})
            msg_id = body.get("id")

            if method_name == "tools/list":
                api_key = headers.get("authorization", "").removeprefix("Bearer ")
                tools = await self.gateway.list_tools(api_key=api_key or None)
                return {
                    "status": 200,
                    "headers": {"content-type": "application/json"},
                    "body": json.dumps({
                        "jsonrpc": "2.0",
                        "id": msg_id,
                        "result": {"tools": tools},
                    }).encode(),
                }

            elif method_name == "tools/call":
                tool_name = params.get("name", "")
                arguments = params.get("arguments", {})
                api_key = headers.get("authorization", "").removeprefix("Bearer ")
                result = await self.gateway.call_tool(
                    tool_name=tool_name,
                    arguments=arguments,
                    api_key=api_key or None,
                )
                return {
                    "status": 200,
                    "headers": {"content-type": "application/json"},
                    "body": json.dumps({
                        "jsonrpc": "2.0",
                        "id": msg_id,
                        "result": result,
                    }).encode(),
                }

            elif method_name == "health":
                health = await self.gateway.health()
                return {
                    "status": 200,
                    "headers": {"content-type": "application/json"},
                    "body": json.dumps({
                        "jsonrpc": "2.0",
                        "id": msg_id,
                        "result": health,
                    }).encode(),
                }

            else:
                return {
                    "status": 400,
                    "headers": {"content-type": "application/json"},
                    "body": json.dumps({
                        "jsonrpc": "2.0",
                        "id": msg_id,
                        "error": {"code": -32601, "message": f"Method not found: {method_name}"},
                    }).encode(),
                }

        return {
            "status": 405,
            "headers": {"content-type": "text/plain"},
            "body": b"Method not allowed",
        }

    async def _read_frame(self, reader: asyncio.StreamReader) -> dict | None:
        # method_len (u16)
        buf = await reader.readexactly(2)
        method_len = struct.unpack("!H", buf)[0]

        method_bytes = await reader.readexactly(method_len)
        method = method_bytes.decode()

        # path_len (u32)
        buf = await reader.readexactly(4)
        path_len = struct.unpack("!I", buf)[0]
        path_bytes = await reader.readexactly(path_len)
        path = path_bytes.decode()

        # headers_len (u32)
        buf = await reader.readexactly(4)
        headers_len = struct.unpack("!I", buf)[0]
        headers_bytes = await reader.readexactly(headers_len)
        headers = json.loads(headers_bytes)

        # body_len (u64)
        buf = await reader.readexactly(8)
        body_len = struct.unpack("!Q", buf)[0]
        body = await reader.readexactly(body_len)

        return {
            "method": method,
            "path": path,
            "headers": headers,
            "body": body,
        }

    async def _write_frame(self, writer: asyncio.StreamWriter, response: dict) -> None:
        status = response["status"]
        headers_json = json.dumps(response["headers"]).encode()
        body = response["body"]

        writer.write(struct.pack("!H", status))
        writer.write(struct.pack("!I", len(headers_json)))
        writer.write(headers_json)
        writer.write(struct.pack("!Q", len(body)))
        writer.write(body)
        await writer.drain()


def main() -> None:
    parser = argparse.ArgumentParser(description="Portail MCP sidecar")
    parser.add_argument(
        "--socket",
        default="/run/portail/mcp.sock",
        help="Unix socket path",
    )
    parser.add_argument(
        "--config",
        default=None,
        help="MCP server config file (JSON)",
    )
    args = parser.parse_args()

    gateway = PortailMcpGateway(config_path=args.config)
    server = UnixSocketServer(socket_path=args.socket, gateway=gateway)

    asyncio.run(server.serve())


if __name__ == "__main__":
    main()
