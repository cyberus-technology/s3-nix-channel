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

      tserve.start()

      # TODO Need to create the bucket first.
      # tserve.wait_for_unit("tarball-serve.service")
    '';
  };
}
