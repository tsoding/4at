# 4at

Simple Multi-User Chat. 

*The code has fallen to "Rewrite it in Rust" disease. You can find the legacy Go version in [./legacy-go-version](./legacy-go-version) if you still need it*

## Quick Start

### Server

```console
$ cargo run
Token: <authentication token>
<logs>
```

### Client

```console
$ telnet 127.0.0.1 6969
Token: <type the token from server here>
<type messages>
```
