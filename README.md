# Tarball Serve

This repository contains a small application that serves a S3 bucket
of tarballs via the [Lockable HTTP Tarball
Protocol](https://docs.lix.systems/manual/lix/stable/protocols/tarball-fetcher.html).

There is really nothing to see here yet! We're missing everything from
deployment to authorization.

```console
$ source credentials

$ cargo run -- --bucket ctrl-os-tarballs --endpoint https://nbg1.your-objectstorage.com
```
