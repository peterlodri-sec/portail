{ config, lib, pkgs, ... }:

let
  cfg = config.services.portail;
  inherit (lib) mkIf mkEnableOption mkOption types;
in
{
  options.services.portail = {
    enable = mkEnableOption "Portail — unified proxy/gateway";

    package = mkOption {
      type = types.package;
      description = "portail package to use";
    };

    mcpPlugin = mkOption {
      type = types.package;
      description = "portail-mcp Python plugin package";
    };

    listen = mkOption {
      type = types.str;
      default = "0.0.0.0:8787";
      description = "Portail listen address";
    };

    cacheDir = mkOption {
      type = types.str;
      default = "/var/cache/portail";
      description = "CDN cache directory";
    };

    cacheSize = mkOption {
      type = types.str;
      default = "10g";
      description = "CDN cache max size";
    };

    enableAiGateway = mkOption {
      type = types.bool;
      default = true;
      description = "Enable AI Gateway subsystem (proxies to LiteLLM upstream)";
    };

    enableMcp = mkOption {
      type = types.bool;
      default = true;
      description = "Enable MCP Gateway subsystem (proxies to Python sidecar)";
    };

    enableCdn = mkOption {
      type = types.bool;
      default = false;
      description = "Enable CDN cache subsystem";
    };

    aiUpstream = mkOption {
      type = types.str;
      default = "http://127.0.0.1:4000";
      description = "AI gateway upstream URL (LiteLLM or compatible)";
    };

    cdnOrigin = mkOption {
      type = types.str;
      default = "http://127.0.0.1:9000";
      description = "CDN origin (MinIO or S3-compatible)";
    };

    natsUrl = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = "NATS URL for CDN cache invalidation";
    };

    mcpConfig = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = "Path to MCP server config JSON";
    };

    cdnDomains = mkOption {
      type = types.listOf types.str;
      default = [];
      description = "CDN domain names (e.g., cdn.example.com)";
    };

    environment = mkOption {
      type = types.attrsOf types.str;
      default = {};
      description = "Extra environment variables";
    };

    openFirewall = mkOption {
      type = types.bool;
      default = false;
      description = "Open the portail port in the firewall";
    };

    memoryMax = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = "Systemd MemoryMax limit (e.g. 512M)";
    };

    tasksMax = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = "Systemd TasksMax limit";
    };
  };

  config = mkIf cfg.enable {
    users.users.portail = {
      isSystemUser = true;
      group = "portail";
      home = "/var/lib/portail";
      createHome = true;
      description = "Portail daemon user";
    };
    users.groups.portail = {};

    systemd.tmpfiles.rules = [
      "d ${cfg.cacheDir} 0750 portail portail - -"
      "d /var/lib/portail 0750 portail portail - -"
      "d /run/portail 0755 portail portail - -"
    ];

    # ── Portail main service (Rust binary) ──────────────────
    systemd.services.portail = {
      description = "Portail — unified proxy/gateway";
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];
      wantedBy = [ "multi-user.target" ];

      serviceConfig = {
        User = "portail";
        Group = "portail";
        ExecStart = "${cfg.package}/bin/portail";
        Restart = "on-failure";
        RestartSec = "5s";
        StateDirectory = "portail";
        CacheDirectory = "portail";
        RuntimeDirectory = "portail";
        RuntimeDirectoryPreserve = "yes";

        # ── Hardening ──────────────────────────────────────
        NoNewPrivileges = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        PrivateTmp = true;
        ProtectClock = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectKernelLogs = true;
        ProtectControlGroups = true;
        PrivateDevices = true;
        LockPersonality = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
        CapabilityBoundingSet = [ "" ];
        SystemCallArchitectures = "native";
        SystemCallFilter = [ "@system-service" "~@privileged" ];
        IPAddressDeny = "any";

        # ── Resource limits ────────────────────────────────
        OOMScoreAdjust = -500;
      } // lib.optionalAttrs (cfg.memoryMax != null) {
        MemoryMax = cfg.memoryMax;
      } // lib.optionalAttrs (cfg.tasksMax != null) {
        TasksMax = cfg.tasksMax;
      };

      environment = {
        PORTAIL_SILENCE_SIGTERM = "1";
      } // lib.optionalAttrs (cfg.enableAiGateway) {
        PORTAIL_ENABLE_AI_GATEWAY = "true";
        PORTAIL_AI_UPSTREAM = cfg.aiUpstream;
        PORTAIL_LISTEN = cfg.listen;
      } // lib.optionalAttrs (cfg.enableMcp) {
        PORTAIL_ENABLE_MCP = "true";
        PORTAIL_MCP_SOCKET = "/run/portail/mcp.sock";
      } // lib.optionalAttrs (cfg.enableCdn) {
        PORTAIL_ENABLE_CDN = "true";
        PORTAIL_CACHE_DIR = cfg.cacheDir;
        PORTAIL_CACHE_SIZE = cfg.cacheSize;
        PORTAIL_CDN_ORIGIN = cfg.cdnOrigin;
      } // lib.optionalAttrs (cfg.natsUrl != null) {
        PORTAIL_NATS_URL = cfg.natsUrl;
      } // cfg.environment;

      preStop = ''
        /run/current-system/sw/bin/kill -TERM $MAINPID
      '';
    };

    # ── MCP sidecar service (Python) ────────────────────────
    systemd.services.portail-mcp = mkIf cfg.enableMcp {
      description = "Portail MCP Gateway — Python sidecar";
      after = [ "network-online.target" ];
      bindsTo = [ "portail.service" ];
      wantedBy = [ "multi-user.target" ];

      serviceConfig = {
        User = "portail";
        Group = "portail";
        ExecStart = "${cfg.mcpPlugin}/bin/portail-mcp --socket /run/portail/mcp.sock${lib.optionalString (cfg.mcpConfig != null) " --config ${cfg.mcpConfig}"}";
        Restart = "on-failure";
        RestartSec = "5s";
        RuntimeDirectory = "portail";
        RuntimeDirectoryPreserve = "yes";

        # ── Hardening ──────────────────────────────────────
        NoNewPrivileges = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        PrivateTmp = true;
        ProtectClock = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectKernelLogs = true;
        ProtectControlGroups = true;
        PrivateDevices = true;
        LockPersonality = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
        CapabilityBoundingSet = [ "" ];
        SystemCallArchitectures = "native";
        SystemCallFilter = [ "@system-service" "~@privileged" ];
        IPAddressDeny = "any";
        OOMScoreAdjust = -500;
      };

      environment = {
        PORTAIL_SILENCE_SIGTERM = "1";
        PORTAIL_MCP_SOCKET = "/run/portail/mcp.sock";
      } // cfg.environment;
    };

    # ── Firewall ────────────────────────────────────────────
    networking.firewall = mkIf cfg.openFirewall {
      allowedTCPPorts = [
        (lib.toInt (builtins.elemAt (lib.splitString ":" cfg.listen) 1))
      ];
    };
  };
}
