# 4at

A Simple Multi-User Chat Application.

*Note: The code has fallen victim to the "Rewrite it in Rust" disease. You can find the legacy Go version in [./legacy-go-version](./legacy-go-version) if you still need it.*

## Quick Start

### Server

To start the server, run:

```console
$ cargo run --bin server
```

Upon running the server, a file named `./TOKEN` will be created, containing the authentication token. You'll need this token to connect to the server using the client.

### Client

To start the client, run:

```console
$ cargo run --bin client
```

Once you are in the client's prompt, use the following command to connect to the server:

```console
> /connect <server ip> <token>
```

This should get you started with 4at! Enjoy chatting with multiple users.
