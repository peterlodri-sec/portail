{ lib, python3, fetchFromGitHub }:

python3.pkgs.buildPythonApplication {
  pname = "portail-mcp";
  version = "0.1.0";

  src = ../plugins/portail-mcp;

  pyproject = true;
  build-system = [ python3.pkgs.setuptools ];

  dependencies = with python3.pkgs; [
    litellm
    fastapi
    uvicorn
    httpx
    pydantic
    mcp
  ];

  meta = {
    description = "Portail MCP Gateway — LiteLLM MCP manager sidecar";
    license = lib.licenses.mit;
  };
}
