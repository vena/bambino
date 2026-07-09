---
paths:
  - "src/client/connect.rs"
  - "src/client/camera.rs"
  - "src/ftps/client.rs"
  - "src/io/tokio.rs"
---

TLS identity uses the printer's serial, not its IP — every TLS call site passes `serial` as the `host`/SNI argument to `TlsConnector::connect()` (the raw stream is already dialed by IP separately via `RawStreamFactory::dial()`). Real printer leaf certs are X.509 **v1** with the serial in Subject CN and no SAN — `rustls-webpki` unconditionally rejects any non-v3 cert, so the tokio "verified" path uses `bambino::io::tokio::CnFallbackServerVerifier` instead: `x509-parser` for all parsing, plus independent chain-of-trust and handshake-signature checks. Identity is SAN-then-CN. `esp-idf`/`embassy` (mbedtls-backed, no v3-only policy) needed no equivalent verifier.
