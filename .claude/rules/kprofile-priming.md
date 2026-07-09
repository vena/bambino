---
paths:
  - "src/diagnostics/kprofile.rs"
  - "src/client/ams.rs"
---

K-profile priming quirk (see doc comment on `ExtrusionCaliGetRequest` in `kprofile.rs` for why): `PrinterClient::get_k_profiles()` auto-primes; opt out via `set_k_profile_primed(true)`.
