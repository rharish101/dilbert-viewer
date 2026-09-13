# SPDX-FileCopyrightText: 2026 Harish Rajagopal <harish.rajagopals@gmail.com>
#
# SPDX-License-Identifier: AGPL-3.0-or-later
packages:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.dilbert-viewer;
  system = pkgs.stdenv.hostPlatform.system;
in
{
  options.services.dilbert-viewer = {
    enable = lib.mkEnableOption "Dilbert Viewer";
    package = lib.mkOption {
      description = "The dilbert-viewer package to use.";
      type = lib.types.package;
      default = packages.${system}.default;
    };
    environmentFile = lib.mkOption {
      description = "The file containing environment variables. Use this to pass secrets, like the database URL.";
      type = lib.types.str;
    };
    logLevel = lib.mkOption {
      description = "The log level for Dilbert Viewer's logs.";
      type = lib.types.str;
      default = "warn";
    };
    host = lib.mkOption {
      description = "The host that Dilbert Viewer will listen on.";
      type = lib.types.str;
      default = "localhost";
    };
    port = lib.mkOption {
      description = "The port that Dilbert Viewer will listen on.";
      type = lib.types.port;
      default = 5000;
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.services.dilbert-viewer = {
      description = "Simple viewer webpage for Dilbert by Scott Adams";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];

      serviceConfig = {
        EnvironmentFile = cfg.environmentFile;
        ExecStart = "${cfg.package}/bin/dilbert-viewer --log-level ${cfg.logLevel} serve --host ${cfg.host} --port ${toString cfg.port} --static-dir ${cfg.package}/share/dilbert-viewer/static";
        Restart = "on-failure";
        RestartSec = 5;

        # Hardening
        CapabilityBoundingSet = "";
        NoNewPrivileges = true;
        PrivateDevices = true;
        PrivateMounts = true;
        PrivateTmp = true;
        PrivateUsers = true;
        ProtectClock = true;
        ProtectControlGroups = true;
        ProtectHome = true;
        ProtectHostname = true;
        ProtectKernelLogs = true;
        ProtectKernelModules = true;
        ProtectKernelTunables = true;
        ProtectProc = "invisible";
        ProtectSystem = "strict";
        RestrictAddressFamilies = [
          "AF_INET"
          "AF_INET6"
          "AF_UNIX"
        ];
        RestrictNamespaces = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
      };
    };
  };
}
