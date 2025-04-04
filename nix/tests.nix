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

  tarball = pkgs.runCommand "tarball-1234.tar.xz"
    {
      nativeBuildInputs = [ pkgs.libarchive ];
    } ''
    mkdir foo
    touch foo/hello

    tar -cJf $out foo
  '';

  tarballServeCommon = {
    services.tarball-serve = {
      enable = true;
      secretsFile = "${secretsFile}";
      listen = "0.0.0.0:3000";

      inherit bucket;
    };
  };

  rsaKeypair = pkgs.runCommand "rsa-keypair"
    {
      nativeBuildInputs = [
        pkgs.openssl
        pkgs.openssh
        pkgs.jwt-cli
      ];
    } ''
    mkdir -p $out
    ssh-keygen -t rsa -b 4096 -E SHA256 -m PEM -P "" -f $out/private.pem
    openssl rsa -pubout -in $out/private.pem -out $out/public.pem

    jwt encode --alg RS256 --exp=100y -S @$out/private.pem > $out/jwt
  '';
in
{
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

      servePublic = { config, pkgs, ... }: {
        imports = [
          self.nixosModules.default
          tarballServeCommon
        ];

        services.tarball-serve = {
          baseUrl = "http://servePublic:3000";
        };
      };

      servePrivate = { config, pkgs, ... }: {
        imports = [
          self.nixosModules.default
          tarballServeCommon
        ];

        services.tarball-serve = {
          baseUrl = "http://servePrivate:3000";
          jwtPublicKey = "${rsaKeypair}/public.pem";
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

      ## Start our server that doesn't require authentication.
      servePublic.start()
      servePublic.wait_for_unit("tarball-serve.service")

      servePublic.succeed("curl -vL http://localhost:3000/channel/thechannel-24.05.tar.xz > latest.tar.xz")
      servePublic.succeed("curl -vL http://localhost:3000/permanent/tarball-1234.tar.xz > permanent.tar.xz")

      servePublic.copy_from_host("${tarball}", "reference.tar.xz")
      servePublic.succeed("cmp reference.tar.xz latest.tar.xz")
      servePublic.succeed("cmp reference.tar.xz permanent.tar.xz")
      servePublic.shutdown()

      ## Start our server that requires authentication
      servePrivate.start()
      servePrivate.wait_for_unit("tarball-serve.service")

      # Unauthorized requests are rejected.
      assert "401" == servePrivate.succeed("curl -s -o /dev/null -w '%{http_code}' http://localhost:3000/channel/thechannel-24.05.tar.xz")
      assert "401" == servePrivate.succeed("curl -s -o /dev/null -w '%{http_code}' http://localhost:3000/permanent/tarball-1234.tar.xz")

      # Authorized accesses succeed.
      servePrivate.copy_from_host("${rsaKeypair}/jwt", "jwt")
      assert "200" == servePrivate.succeed("curl -Ls -u :$(cat jwt) --basic -o /dev/null -w \'%{http_code}\' http://localhost:3000/channel/thechannel-24.05.tar.xz")
      assert "200" == servePrivate.succeed("curl -Ls -u :$(cat jwt) --basic -o /dev/null -w \'%{http_code}\' http://localhost:3000/permanent/tarball-1234.tar.xz")
    '';

    # TODO Add test for using the tarball as a flake input.
  };
}
