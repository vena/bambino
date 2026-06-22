//! # P1 Series (P1P & P1S CoreXY) Quirks
//!
//! Tracks constraints of early and enclosed low-power RTOS machines.
//!
//! **Post-Boot Delay Quirk [REF-NET-SECURE]:**
//! ESP32-based RTOS boards exhibit high cryptographic latency, requiring up to
//! 30 seconds after hardware boot to load the MQTTS broker certificates.
//! Handshake timeout budgets must be scaled dynamically to prevent connection drops.

/// Standard post-boot socket preparation delay, in seconds
pub const POST_BOOT_CONNECT_DELAY: u64 = 25;

/// Connection handshake timeout limits specifically configured for low-resource ESP32 platforms
pub const CRYPTO_HANDSHAKE_TIMEOUT_MS: u64 = 5000;
