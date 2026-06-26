{ lib, python3, fetchFromGitHub }:

python3.pkgs.buildPythonApplication {
  pname = "portail-mcp";
  version = "0.1.0";

  src = ../plugins/portail-mcp;

  pyproject = true;
  build-system = [ python3.pkgs.setuptools ];

  dependencies = with python3.pkgs; [
    httpx
    fastapi
    uvicorn
    pydantic
  ] ++ lib.optionals (builtins.pathExists ../.litellm-path) [
    # LiteLLM — added as an optional dep when available
  ];

  meta = {
    description = "Portail MCP Gateway — LiteLLM MCP manager sidecar";
    license = lib.licenses.mit;
  };
}
