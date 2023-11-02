# 4at

Simple Multi-User Chat. 

*The code has fallen to "Rewrite it in Rust" disease. You can find the legacy Go version in [./legacy-go-version](./legacy-go-version) if you still need it*

## Quick Start

### Server

```console
$ cargo run --bin server
Token: <authentication token>
<logs>
```

### Client

```console
$ cargo run --bin client 127.0.0.1
<type the token from server>
<type messages>
```
