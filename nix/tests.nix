{ self, system }:
let
  pkgs = self.inputs.nixpkgs.legacyPackages.${system};

  accessKey = "12341234";
  secretKey = "abcdabcd";
  region = "eu-central-1";
  bucket = "bucket";

  secretsFile = pkgs.writeText "fake-secrets" ''
    AWS_ACCESS_KEY_ID=${accessKey}
    AWS_SECRET_ACCESS_KEY=${secretKey}
    AWS_REGION=${region}
    AWS_ENDPOINT_URL="http://s3:9000"
  '';

  channelsConfig = pkgs.writeText "channels.json" (builtins.toJSON {
    channels = [ "thechannel-24.05" ];
  });

  thechannelConfig = pkgs.writeText "thechannel-24.05.json" (builtins.toJSON {
    latest = "tarball-1234";
  });

  tarball = pkgs.runCommand "tarball-1234.tar.xz" {
    buildInputs = [ pkgs.libarchive ];
  } ''
    mkdir foo
    touch foo/hello

    tar -cJf $out foo
  '';
in {
  canServeFiles = pkgs.nixosTest {
    name = "tarball-serve";

    nodes = {
      s3 = { config, ... }: {
        services.minio = {
          inherit accessKey secretKey region;

          enable = true;
          # minio listens by default on port 9000.
        };

        environment.systemPackages = with pkgs; [
          minio-client
        ];

        networking.firewall.enable = false;
      };

      tserve = { config, pkgs, ... }: {
        imports = [
          self.nixosModules.default
        ];

        services.tarball-serve = {
          enable = true;
          secretsFile = "${secretsFile}";
          listen = "0.0.0.0:3000";
          baseUrl = "http://tserve:3000";

          inherit bucket;
        };
      };
    };

    testScript = ''
      s3.start()
      s3.wait_for_unit("minio.service")

      ## Prepare the bucket of tarballs with configuration.

      # Minio sometimes takes a second to come up.
      s3.wait_until_succeeds("mc alias set local http://localhost:9000 ${accessKey} ${secretKey}")
      s3.succeed("mc mb local/${bucket}")

      s3.succeed("mkdir content")
      s3.copy_from_host("${channelsConfig}", "content/channels.json");
      s3.copy_from_host("${thechannelConfig}", "content/thechannel-24.05.json");
      s3.copy_from_host("${tarball}", "content/tarball-1234.tar.xz");

      s3.succeed("mc cp content/* local/${bucket}/")

      ## Start our server.
      tserve.start()
      tserve.wait_for_unit("tarball-serve.service")

      tserve.succeed("curl -L http://localhost:3000/channel/thechannel-24.05.tar.xz > latest.tar.xz")
      tserve.succeed("curl -L http://localhost:3000/permanent/tarball-1234.tar.xz > permanent.tar.xz")

      tserve.copy_from_host("${tarball}", "reference.tar.xz")
      tserve.succeed("cmp reference.tar.xz latest.tar.xz")
      tserve.succeed("cmp reference.tar.xz permanent.tar.xz")
    '';
  };
}
