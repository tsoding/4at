# 4at

Simple Multi-User Chat.

*The code has fallen to "Rewrite it in Rust" disease. You can find the legacy Go version in [./legacy-go-version](./legacy-go-version) if you still need it*

## Quick Start

### Server

```console
$ cargo run --bin server
```

Upon running the server creates `./TOKEN` where the Authentication Token is located. You will needed to connect to the Server via the Client.

### Client

```console
$ cargo run --bin client
```

In the prompt of the Client

```console
> /connect <server ip> <token>
```
