//! Re-emits ESP-IDF's kconfig options as `--cfg esp_idf_*` for this crate.
//!
//! `esp-idf-sys`'s build script turns the target's sdkconfig into a set of cfg flags, and
//! `esp-idf-svc` (which owns the `esp_idf_svc` `links` key) propagates them to its direct
//! dependents as build-script metadata. Cargo only hands those to a dependent that *has* a
//! build script, and only that dependent's own build script can turn them into `--cfg`
//! flags for its crate. So without this file, `#[cfg(esp_idf_mbedtls_certificate_bundle)]`
//! in `io/esp_idf.rs` is not an error — it is silently always false, and the code it guards
//! never compiles in. That failure mode (green build, missing behavior) is why the gate is
//! backed by a build script rather than assumed to work; see GitHub issue #62.
//!
//! Only `output()` is called, never `sysenv::relay()`: relay emits `cargo:KEY=VAL`
//! metadata, which Cargo rejects for a package with no `links` key of its own.
//!
//! Nothing here runs on host/embassy builds — `embuild` is an optional build-dependency
//! enabled only by the `esp-idf` feature.

fn main() {
    // Declared unconditionally so the `unexpected_cfgs` lint accepts the gate in
    // `io/esp_idf.rs` on every target, not just the one where the cfg is actually emitted.
    // embuild emits `rustc-cfg` without a matching `rustc-check-cfg`, so this has to be
    // stated here.
    println!("cargo::rustc-check-cfg=cfg(esp_idf_mbedtls_certificate_bundle)");

    #[cfg(feature = "esp-idf")]
    ::embuild::espidf::sysenv::output();
}
