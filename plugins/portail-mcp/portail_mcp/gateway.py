"""
Portail MCP Gateway — wraps LiteLLM's MCPServerManager.

LiteLLM's MCP server manager handles:
- Server registration (config-based + DB-based)
- Tool prefixing (collision avoidance)
- Auth/permission filtering
- Multi-transport support (SSE, Streamable HTTP, stdio)
- Tool call routing to upstream MCP servers

This module extracts the relevant parts and exposes them as a
lightweight ASGI app that Portail's Rust front-end proxies to.
"""

from __future__ import annotations

import json
import logging
from pathlib import Path
from typing import Any

import httpx

logger = logging.getLogger("portail.mcp.gateway")

# ── LiteLLM MCP imports (extracted) ────────────────────────
# In the full Portail build, these would come from a litellm
# dependency. The MCPServerManager is the core orchestrator.
try:
    from litellm.proxy._experimental.mcp_server.mcp_server_manager import (
        MCPServerManager,
    )
    from litellm.proxy._experimental.mcp_server.server import (
        handle_mcp_request,
    )

    LITELLM_AVAILABLE = True
except ImportError:
    LITELLM_AVAILABLE = False
    MCPServerManager = object  # type: ignore
    logger.warning("litellm not available — MCP gateway running in stub mode")


class PortailMcpGateway:
    """Wraps LiteLLM's MCP server manager for Portail.

    Architecture:
        Portail Rust proxy  ──Unix socket──▶  PortailMcpGateway
                                                  │
                                          ┌───────┴────────┐
                                          │  MCPServerManager │
                                          │  (LiteLLM)        │
                                          └───────┬────────┘
                                                  │
                                    ┌─────────────┼─────────────┐
                                    │             │             │
                                SSE server   stdio proc   HTTP MCP
    """

    def __init__(self, config_path: str | None = None):
        self.config_path = config_path
        self.manager: MCPServerManager | None = None
        self._server_config: dict[str, Any] = {}
        self._initialised = False

    async def initialise(self) -> None:
        """Load MCP server registry from config."""
        if not LITELLM_AVAILABLE:
            logger.info("MCP gateway running in stub mode — no Litellm available")
            self._initialised = True
            return

        self.manager = MCPServerManager()

        # Load servers from config file if provided
        if self.config_path and Path(self.config_path).exists():
            with open(self.config_path) as f:
                self._server_config = json.load(f)

            mcp_servers = self._server_config.get("mcp_servers", {})
            for name, server_cfg in mcp_servers.items():
                await self.manager.add_server(
                    server_name=name,
                    server_config=server_cfg,
                )
                logger.info("registered MCP server: %s", name)

        # Load from bundled defaults
        # (future: config-driven default servers)
        pass

        self._initialised = True
        logger.info(
            "MCP gateway initialised: %d servers",
            len(self.manager.registry) if self.manager else 0,
        )

    def _register_default_servers(self) -> None:
        """Register the default Portail MCP servers.

        Override in subclasses or via config to add pre-registered
        MCP servers (e.g. filesystem, docs search, memory).
        """

    async def list_tools(self, api_key: str | None = None) -> list[dict[str, Any]]:
        """List all available MCP tools (with auth filtering)."""
        if not self.manager or not self._initialised:
            return self._stub_tools()

        try:
            tools = await self.manager.list_tools(api_key=api_key)
            return [t.model_dump() for t in tools]
        except Exception as e:
            logger.error("failed to list MCP tools: %s", e)
            return self._stub_tools()

    async def call_tool(
        self,
        tool_name: str,
        arguments: dict[str, Any],
        api_key: str | None = None,
    ) -> dict[str, Any]:
        """Call a specific MCP tool."""
        if not self.manager or not self._initialised:
            return {"error": "MCP gateway not initialised"}

        try:
            result = await self.manager.call_tool(
                tool_name=tool_name,
                arguments=arguments,
                user_api_key=api_key,
            )
            return {"result": result}
        except Exception as e:
            logger.error("failed to call MCP tool %s: %s", tool_name, e)
            return {"error": str(e)}

    async def health(self) -> dict[str, Any]:
        """Health check."""
        return {
            "status": "ok" if self._initialised else "starting",
            "litellm_available": LITELLM_AVAILABLE,
            "servers": len(self.manager.registry) if self.manager else 0,
        }

    def _stub_tools(self) -> list[dict[str, Any]]:
        """Return stub tools when LiteLLM is not available."""
        return [
            {
                "name": "portail_info",
                "description": "Portail MCP gateway information",
                "inputSchema": {"type": "object", "properties": {}},
            }
        ]
