# Protocol Specifications

Protocol specifications referenced by the connectrpc implementation. The spec
documents themselves are **not committed** (third-party content) — fetch them
locally with `task specs:fetch`.

## Files

- **connect-protocol.md** - The [Connect Protocol Reference](https://connectrpc.com/docs/protocol/) from connectrpc.com. This is the primary specification for the Connect protocol.

- **grpc-http2-protocol.md** - The [gRPC over HTTP/2 Protocol](https://github.com/grpc/grpc/blob/master/doc/PROTOCOL-HTTP2.md) specification. The native gRPC protocol that Connect is designed to interoperate with.

- **grpc-web-protocol.md** - The [gRPC-Web Protocol](https://github.com/grpc/grpc/blob/master/doc/PROTOCOL-WEB.md) specification. Connect servers can also support gRPC-Web clients.

## Sources

- Connect Protocol: https://github.com/connectrpc/connectrpc.com/blob/main/docs/protocol.md
- gRPC HTTP/2 Protocol: https://github.com/grpc/grpc/blob/master/doc/PROTOCOL-HTTP2.md
- gRPC-Web Protocol: https://github.com/grpc/grpc/blob/master/doc/PROTOCOL-WEB.md

## Related Specifications

For a complete implementation, you may also need to reference:

- [Protocol Buffers](https://protobuf.dev/) - Message serialization format
- [Proto3 JSON Mapping](https://protobuf.dev/programming-guides/proto3/#json) - Canonical JSON encoding for protobuf messages
- [HTTP/2 RFC 7540](https://tools.ietf.org/html/rfc7540) - HTTP/2 framing specification

## Fetching

```sh
task specs:fetch
```

This pulls the latest versions from the source repositories via the GitHub
API. Re-run to refresh.
