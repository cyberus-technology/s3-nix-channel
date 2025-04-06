# S3 Nix Channel Server

A Rust service that serves an S3 bucket via the [Nix Lockable Tarball
Protocol](https://nix.dev/manual/nix/2.25/protocols/tarball-fetcher).

## Overview

This service enables you to host Nix channels using an S3 bucket as
the storage backend. It implements the Nix Lockable HTTP Tarball
Protocol to allow these tarballs to be used as Flake inputs.

## Features

- Supports multiple channels with different versions.
- Let's S3 serve the actual tarballs.
- Periodically refreshes channel configuration without restarts.
- Authentication via [JWT](https://en.wikipedia.org/wiki/JSON_Web_Token)

## How It Works

The service provides two main endpoints:

- `/channel/{channel-name}.tar.xz` - Redirects to the latest version of a channel.
- `/permanent/{object-key}.tar.xz` - Serves a specific immutable tarball by key.

See below for the correct S3 bucket layout and usage instructions.

## Installation

### Building from Source

This is a normal Rust program without any special dependencies. Refer
to the [Cargo documentation](https://doc.rust-lang.org/cargo/).

## Usage

For S3 buckets that need authentication, you must set
`AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` in the environment.

You can serve from AWS S3 using this command:

```bash
s3-nix-channel \
  --endpoint https://s3.amazonaws.com \
  --bucket your-nix-channel-bucket \
  --base-url https://example.com \
  --listen 0.0.0.0:3000
```

To serve from Hetzner Object Storage you need to set `AWS_REGION` and `AWS_ENDPOINT_URL` in the environment as well. This could look like:

```bash
export AWS_ACCESS_KEY_ID=<your-access-key>
export AWS_SECRET_ACCESS_KEY=<your-secret-key>
export AWS_REGION="eu-central-1"
export AWS_ENDPOINT_URL="https://nbg1.your-objectstorage.com"
```

### Authentication

If authentication is required, `s3-nix-channel` can be started with
`--jwt-pem public.pem`, where `public.pem` is a RSA public key.  We
currently support the `RS256` algorithm.

Incoming requests must then use HTTP Basic authentication with a
signed token in the _password_ part of the request.

## S3 Bucket Configuration

### channels.json

This file defines the available channels. Example:

```json
{
  "channels": ["nixos-25.05", "nixos-unstable"]
}
```

### \<channel-name\>.json

Each channel needs its own configuration file. Example for `nixos-25.05.json`:

```json
{
  "latest": "nixos-25.05-2025-05-15"
}
```

This means requests to `/channel/nixos-25.05.tar.xz` will redirect to
the tarball at `/permanent/nixos-25.05-2025-05-15.tar.xz`, with
appropriate immutable link headers.

## Nix Flake Configuration

To use a channel as a Nix Flake input, refer to the `/channel` endpoint:

```nix
  inputs.example.url = "https://example.com/channel/nixos-25.05.tar.xz";
```

## License

See the `LICENSE` file.
