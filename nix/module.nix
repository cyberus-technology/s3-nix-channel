{ config, lib, pkgs, ... }:
let
  cfg = config.services.tarball-serve;
in {
  options.services.tarball-serve = {
    enable = lib.mkEnableOption "Enables the tarball-serve service";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.tarball-serve;
    };

    bucket = lib.mkOption {
      type = lib.types.str;
      description = "The name of the S3 bucket to serve.";
    };

    baseUrl = lib.mkOption {
      type = lib.types.str;
      default = "http://localhost:3000";
      description = "The base URL of the server. This is used for proper redirects.";
    };

    listen = lib.mkOption {
      type = lib.types.str;
      default = "[::]:3000";
      description = ''
        Where to listen for connections. See
        https://www.freedesktop.org/software/systemd/man/systemd.socket.html#ListenStream=
        for more information.
      '';
    };

    secretsFile = lib.mkOption {
      type = lib.types.path;
      description = ''

        Path of a file containing S3 configuration in the format of
        EnvironmentFile as described by {manpage}`systemd.exec(5)`.

        For example:

        ```
        AWS_ACCESS_KEY_ID=...
        AWS_SECRET_ACCESS_KEY=..
        AWS_REGION=...
        AWS_ENDPOINT_URL=...
        ```
      '';
    };

    jwtPublicKey = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = ''
        Path to a RSA public key in PEM file. If this option is specified,
        the service will require a valid JWT token to respond to requests.
        JWT tokens must be sent via the password field of HTTP Basic Auth.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.sockets.tarball-serve = {
      wantedBy = [ "sockets.target" ];
      socketConfig = {
        ListenStream = cfg.listen;
        Accept = "false";
      };
    };

    systemd.services.tarball-serve = {
      description = "Nix Tarball Serve";
      wantedBy = [ "multi-user.target" ];

      after = [ "network.target" ];
      requires = [ "tarball-serve.socket" ];

      serviceConfig = {
        Type = "notify";
        NotifyAccess = "main";

        ExecStart = ''
          ${lib.getExe cfg.package} \
            --bucket ${cfg.bucket}  \
            --base-url ${cfg.baseUrl} \
            ${lib.optionalString (cfg.jwtPublicKey != null)
              "--jwt-pem \${CREDENTIALS_DIRECTORY}/pem"}
        '';

        # TODO Hardening

        EnvironmentFile = cfg.secretsFile;
      } // lib.optionalAttrs (cfg.jwtPublicKey != null) {
        LoadCredential = "pem:${cfg.jwtPublicKey}";
      };
    };
  };
}
