# 🚀 S3 Nix Channel Server

A Rust service that serves an S3 bucket via the [Nix Lockable HTTP
Tarball
Protocol](https://nix.dev/manual/nix/2.25/protocols/tarball-fetcher),
allowing you to host your own Nix channels effortlessly.

## 📖 Overview

This service enables you to host Nix channels using an S3-compatible
storage backend. It implements the Nix Lockable HTTP Tarball Protocol
to allow these tarballs to be used as Flake inputs.

## ✨ Features

- 📦 Supports multiple channels with different versions
- 🔄 Lets S3 serve the actual tarballs for efficiency
- 🔁 Periodically refreshes channel configuration without restarts
- 🔒 Authentication via [JWT](https://en.wikipedia.org/wiki/JSON_Web_Token) (optional)
- 🛡️ Strong sandboxing via systemd (when using the Nix module)

## 🛠️ How It Works

The service provides two main endpoints:

- `/channel/{channel-name}.tar.xz` - Redirects to the latest version of a channel
- `/permanent/{object-key}.tar.xz` - Serves a specific immutable tarball by key

### 📝 Nix Flake Configuration

To use a channel as a Nix Flake input:

```nix
{
  inputs = {
    example.url = "https://example.com/channel/nixos-25.05.tar.xz";
  };

  # ...
}
```

## 🔧 Installation

### Building from Source

```bash
# Clone the repository
$ git clone https://github.com/blitz/s3-nix-channel.git
$ cd s3-nix-channel

# Build with cargo
$ cargo build --release
```

The binary will be available at `target/release/s3-nix-channel`.

## 🚀 Usage

### AWS S3

For S3 buckets that need authentication, set these environment variables:

```bash
export AWS_ACCESS_KEY_ID=<your-access-key>
export AWS_SECRET_ACCESS_KEY=<your-secret-key>
```

Start the server:

```bash
s3-nix-channel \
  --bucket your-nix-channel-bucket \
  --base-url https://example.com \
  --listen 0.0.0.0:3000
```

### Hetzner Object Storage

For Hetzner Object Storage, set these additional environment
variables:

```bash
export AWS_ACCESS_KEY_ID=<your-access-key>
export AWS_SECRET_ACCESS_KEY=<your-secret-key>
export AWS_REGION="eu-central-1"
export AWS_ENDPOINT_URL="https://nbg1.your-objectstorage.com"
```

Adjust the endpoint URL as necessary. Then start the server as shown
above.

### Other S3-Compatible Providers

Most S3-compatible storage providers should work by setting the
appropriate endpoint and credentials.

## 🔒 Authentication

If authentication is required,
[JWT](https://en.wikipedia.org/wiki/JSON_Web_Token) can be used. The
supported algorithm is `RS256`.

1. Generate an RSA key pair:
   ```bash
   $ openssl genrsa -out private.pem 2048
   $ openssl rsa -in private.pem -pubout -out public.pem
   ```

2. Start the server with the public key:
   ```bash
   $ s3-nix-channel \
     --bucket your-nix-channel-bucket \
     --base-url https://example.com \
     --listen 0.0.0.0:3000 \
     --jwt-pem public.pem
   ```

3. For clients, create JWT tokens signed with the private key and use
   HTTP Basic authentication with the token as the password. This is
   designed to be used via the
   [netrc](https://nix.dev/manual/nix/2.25/command-ref/conf-file#conf-netrc-file).

## 📁 S3 Bucket Configuration

### channels.json

This file defines the available channels:

```json
{
  "channels": ["nixos-25.05", "nixos-unstable"]
}
```

### <channel-name>.json

Each channel needs its own configuration file. Example for
`nixos-25.05.json`:

```json
{
  "latest": "nixos-25.05-2025-05-15"
}
```

This means requests to `/channel/nixos-25.05.tar.xz` will redirect to
the tarball at `/permanent/nixos-25.05-2025-05-15.tar.xz`, with
appropriate immutable link headers.

### Updating Channels

New tarballs can be uploaded with `s3-nix-channel-upload`. You'll need
to configure authentication via environment variables (see above).

```bash
s3-nix-channel-upload publish your-nix-channel-bucket nixos-25.05 nixos-25.05-2025-05-20.tar.xz
```

## 👥 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## 📜 License

See the `LICENSE` file.
