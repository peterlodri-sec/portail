# nix/opencode-mux.nix — flake helper for the opencode multiplexer shell + apps.
#
# Returns `{ shell, appMux, appNoMux }` for flake.nix to wire in.
# Multiplexer JSON template lives at nix/opencode-mux/default.json.
{
  pkgs,
  inputs',
  template ? ./opencode-mux/default.json,
}:
let
  nushellModule = ../../nushell/ohmy-slim.nu;

  sharedPackages = with pkgs; [
    nushell
    zellij
    jq
  ];

  # Wrapper script that runs `ohmy-slim mux-launch` in a clean nushell env.
  muxLauncher = pkgs.writeShellScriptBin "ohmy-mux-launch" ''
    set -euo pipefail
    exec ${pkgs.nushell}/bin/nu -c "use ${nushellModule} *; ohmy-slim mux-launch --port 0"
  '';

  # Wrapper script that runs `ohmy-slim launch` directly (no zellij).
  opencodeMuxLauncher = pkgs.writeShellScriptBin "opencode-mux" ''
    set -euo pipefail
    exec ${pkgs.nushell}/bin/nu -c "use ${nushellModule} *; ohmy-slim launch --port 4096"
  '';

  shell = pkgs.mkShell {
    name = "portail-opencode-mux";
    packages = sharedPackages ++ [
      inputs'.llm-agents.packages.opencode
    ];
    shellHook = ''
      echo "portail-opencode-mux — nushell + zellij + opencode"
      echo "  ohmy-slim mux-launch   # write config + zellij session"
      echo "  opencode-mux           # opencode in current shell (no mux)"
    '';
  };
in
{
  inherit shell;
  appMux = {
    type = "app";
    program = "${muxLauncher}/bin/ohmy-mux-launch";
    meta = {
      description = "Write multiplexer config and launch opencode inside a fresh zellij session";
      mainProgram = "ohmy-mux-launch";
    };
  };
  appNoMux = {
    type = "app";
    program = "${opencodeMuxLauncher}/bin/opencode-mux";
    meta = {
      description = "Launch opencode without the multiplexer (background subagents only)";
      mainProgram = "opencode-mux";
    };
  };
}
