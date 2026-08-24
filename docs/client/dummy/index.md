*[bambino](../../index.md) / [client](../index.md) / [dummy](index.md)*

---

# Module `dummy`

Zero-cost dummy implementations for [`PrinterClient`](../index.md#printerclient)'s type parameters.

These let you create an MQTT-only `PrinterClient` without specifying concrete FTPS,
TLS, or timer types. They're the defaults — you'll never need to reference them directly
unless you're building a fully custom client configuration.

