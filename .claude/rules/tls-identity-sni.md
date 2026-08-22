---
paths:
  - "src/client/connect.rs"
  - "src/client/camera.rs"
  - "src/ftps/client.rs"
  - "src/io/tokio.rs"
---

TLS identity uses the printer's serial, not its IP — every TLS call site passes `serial` as the `host`/SNI argument to `TlsConnector::connect()` (the raw stream is already dialed by IP separately via `RawStreamFactory::dial()`). Real printer leaf certs are X.509 **v1** with the serial in Subject CN and no SAN, and are **not** self-signed — a live P1S (firmware 01.10.00.00) verified end-to-end against the BBL CA anchors, so a real chain of trust exists and `--with-certs` on the CLI is a working verification path, not just a diagnostic. Any doc text claiming the leaf is self-signed is stale — `rustls-webpki` unconditionally rejects any non-v3 cert, so the tokio "verified" path uses `bambino::io::tokio::CnFallbackServerVerifier` instead: `x509-parser` for all parsing, plus independent chain-of-trust and handshake-signature checks. Identity is SAN-then-CN. `esp-idf`/`embassy` (mbedtls-backed, no v3-only policy) needed no equivalent verifier.
