# ELIZA example

A Rust port of the [`connectrpc/examples-go`](https://github.com/connectrpc/examples-go)
ELIZA psychotherapist demo. The proto service contract
(`connectrpc.eliza.v1.ElizaService`) is byte-identical to the Go reference,
so the server and client in this directory interoperate with any other
implementation — including the hosted Go demo at `https://demo.connectrpc.com`.

Three RPCs:

| RPC | Type | What it does |
|---|---|---|
| `Say` | Unary | One sentence in, one sentence out. Idempotent (works via Connect GET). |
| `Introduce` | Server streaming | Eliza sends a few introductory sentences. |
| `Converse` | Bidirectional streaming | Back-and-forth chat. Eliza ends the stream when she detects a goodbye. |

The ELIZA response logic (pattern matching, pronoun reflection, canned responses)
is ported from examples-go's `internal/eliza` package, which itself adapts
[mattshiel/eliza-go](https://github.com/mattshiel/eliza-go).

## Quick start

```bash
# Terminal 1: server (plaintext, listens on 127.0.0.1:8080)
cargo run -p eliza-example --bin eliza-server

# Terminal 2: interactive client
cargo run -p eliza-example --bin eliza-client
```

Type sentences and press Enter. Say `bye`, `quit`, or `goodbye` and Eliza will
gracefully end the conversation — the server sends her farewell then closes
the bidi stream, and the client detects the close and exits.

## Testing with curl

Connect-protocol unary with JSON is just HTTP POST:

```bash
curl -X POST http://localhost:8080/connectrpc.eliza.v1.ElizaService/Say \
  -H "Content-Type: application/json" \
  -d '{"sentence": "I feel happy"}'
```

## Cross-implementation interop

The client speaks to any `connectrpc.eliza.v1.ElizaService` endpoint. With the
`tls` feature enabled, you can point it at the hosted Go reference server:

```bash
cargo run --bin eliza-client --features tls -- --url https://demo.connectrpc.com
```

This proves Rust client ↔ Go server interop over TLS (Connect protocol,
ALPN-negotiated HTTP/2, server-streaming and bidirectional RPCs).

## TLS

Enable the `tls` feature for HTTPS support on both the server and client.

### Server

```bash
# TLS only — no client authentication
cargo run --bin eliza-server --features tls -- \
  --cert server.pem --key server.key

# mTLS — server requires and verifies client certificates
cargo run --bin eliza-server --features tls -- \
  --cert server.pem --key server.key --client-ca client-ca.pem
```

### Client

```bash
# Default trust store (webpki-roots, same as browsers) — works for public CAs
cargo run --bin eliza-client --features tls -- --url https://demo.connectrpc.com

# Custom CA bundle — for self-signed or private CAs
cargo run --bin eliza-client --features tls -- \
  --url https://localhost:8443 --ca server-ca.pem

# mTLS — client presents a certificate
cargo run --bin eliza-client --features tls -- \
  --url https://localhost:8443 --ca server-ca.pem \
  --cert client.pem --key client.key
```

### Certificate requirements

rustls (via webpki) enforces standard PKI rules strictly: the CA certificate
must be distinct from the server leaf certificate. A single self-signed cert
used as both CA and end-entity will be rejected during verification.

For local testing, generate a proper CA → leaf chain:

```bash
# Server CA (signs the server leaf)
openssl req -x509 -newkey rsa:2048 -keyout server-ca.key -out server-ca.pem -days 365 -nodes \
  -subj "/CN=eliza-server-ca" \
  -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign"

# Server leaf cert (signed by the server CA, with SAN for localhost)
openssl req -newkey rsa:2048 -keyout server.key -out server.csr -nodes -subj "/CN=localhost"
openssl x509 -req -in server.csr -CA server-ca.pem -CAkey server-ca.key -out server.pem \
  -days 365 -CAcreateserial \
  -extfile <(printf "subjectAltName=DNS:localhost\nbasicConstraints=CA:FALSE\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n")

# For mTLS, repeat with a separate client CA + client leaf (extendedKeyUsage=clientAuth)
```

The client trusts `server-ca.pem` (`--ca server-ca.pem`); the server presents
`server.pem` + `server.key` (`--cert`/`--key`).

## Flags

Run `--help` on either binary for the full flag list. Key flags:

**Server** (`eliza-server`):
- `--addr` — listen address (default `127.0.0.1:8080`, also reads `ADDR` env). Use `0.0.0.0:8080` (IPv4) or `[::]:8080` (IPv6, also accepts IPv4 on Linux) to bind all interfaces.
- `--stream-delay` — delay between server-stream responses, e.g. `100ms` (default `0s`)
- `--cert` / `--key` — server TLS cert chain + private key (PEM). Enables TLS.
- `--client-ca` — client CA bundle (PEM) for mTLS. Requires `--cert`/`--key`.

**Client** (`eliza-client`):
- `--url` — server URL (default `http://localhost:8080`, also reads `ELIZA_URL` env)
- `--name` — your name for Eliza's introduction (default from `USER` env)
- `--ca` — CA bundle to trust (PEM). If not set, uses webpki-roots.
- `--cert` / `--key` — client cert + key (PEM) for mTLS.

TLS flags are only available when built with `--features tls`.

## IPv6

The server accepts any address format `tokio::net::TcpListener::bind` accepts:

| Address | Binds to |
|---|---|
| `127.0.0.1:8080` | IPv4 loopback only (the default) |
| `[::1]:8080` | IPv6 loopback only |
| `localhost:8080` | Resolved via DNS/hosts — may be v4, v6, or both depending on your system |
| `0.0.0.0:8080` | All IPv4 interfaces |
| `[::]:8080` | All IPv6 interfaces. On Linux with default `net.ipv6.bindv6only=0`, this **also** accepts IPv4 connections via IPv4-mapped addresses (`::ffff:a.b.c.d`) — so a single `[::]` bind covers both stacks. macOS and BSDs typically default to v6-only. |

The client's `--url` flag uses standard URI syntax: `http://[::1]:8080` for
IPv6 literals (brackets required).

## Implementation notes

**`BidiStream` goodbye detection.** After printing each Converse response, the
client does a 100ms timeout-peek on `message()` to check for `END_STREAM`. When
Eliza ends the session (the farewell DATA frame is immediately followed by
END_STREAM), this catches the close before the next stdin prompt. Without the
peek, `read_line()` would block in interactive mode, leaving you at a `You>`
prompt even though the server has already hung up.

**TLS server path uses `Server::with_tls`**, not axum. The plaintext path uses
`axum::serve` (which gives us the `/health` endpoint). The TLS path uses
connectrpc's built-in `Server` which doesn't have per-route configuration —
a trade-off for simplicity. You could add health as an RPC method, or use
`Server::serve_with_service` with a custom dispatcher, if you need both.

**PEM loading** uses `rustls-pemfile` for both server and client. The server
uses `WebPkiClientVerifier` for mTLS; the client uses `with_client_auth_cert()`
for presenting a certificate.
