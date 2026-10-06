//! Every wire bitmask and sentinel the telemetry decoders read, with the small helpers that apply them.
//!
//! Both the per-report decoders on [`PrinterTelemetry`](super::PrinterTelemetry) and the cached
//! accessors on `PrinterClient` call these helpers, so a mask is written down once and the two
//! readings of the same field cannot drift apart.

use crate::quirks::Axis;

// ---------------------------------------------------------------------------
// `home_flag` [REF-HOMEFLAG]
// ---------------------------------------------------------------------------

/// `home_flag` bit 0: X homed.
pub(crate) const HOME_FLAG_X: u32 = 1 << 0;
/// `home_flag` bit 1: Y homed.
pub(crate) const HOME_FLAG_Y: u32 = 1 << 1;
/// `home_flag` bit 2: Z homed.
pub(crate) const HOME_FLAG_Z: u32 = 1 << 2;
/// `home_flag` bits 0-2 together.
pub(crate) const HOME_FLAG_XYZ: u32 = HOME_FLAG_X | HOME_FLAG_Y | HOME_FLAG_Z;
/// `home_flag` bit 3: mains wired for the 220V region.
pub(crate) const HOME_FLAG_POWER_220V: u32 = 1 << 3;
/// Shift to `home_flag`'s two-bit SD-card state field (bits 8-9).
pub(crate) const HOME_FLAG_SDCARD_SHIFT: u32 = 8;
/// Width mask of the SD-card state field once shifted down.
pub(crate) const HOME_FLAG_SDCARD_MASK: u32 = 0x3;
/// Bit 23 of `home_flag` (X1) and of the hex `stat` field (H2/P2/X2): door open [REF-NET-DOOR].
pub(crate) const DOOR_OPEN: u64 = 1 << 23;

/// Whether `axis`'s homed bit is set in `home_flag`.
pub(crate) fn is_axis_homed(home_flag: u32, axis: Axis) -> bool {
    let bit = match axis {
        Axis::X => HOME_FLAG_X,
        Axis::Y => HOME_FLAG_Y,
        Axis::Z => HOME_FLAG_Z,
    };
    home_flag & bit != 0
}

/// Whether X, Y and Z are all homed in `home_flag`.
pub(crate) fn is_all_axes_homed(home_flag: u32) -> bool {
    home_flag & HOME_FLAG_XYZ == HOME_FLAG_XYZ
}

/// Whether `home_flag` reports mains wired for the 220V region.
pub(crate) fn is_220v(home_flag: u32) -> bool {
    home_flag & HOME_FLAG_POWER_220V != 0
}

/// `home_flag`'s two-bit SD-card state field (bits 8-9), shifted down.
pub(crate) fn sdcard_state_bits(home_flag: u32) -> u32 {
    (home_flag >> HOME_FLAG_SDCARD_SHIFT) & HOME_FLAG_SDCARD_MASK
}

/// Whether the door bit is set in a `home_flag` or decoded `stat` value.
pub(crate) fn is_door_open(value: u64) -> bool {
    value & DOOR_OPEN != 0
}

// ---------------------------------------------------------------------------
// Network [REF-NET-PORTS]
// ---------------------------------------------------------------------------

/// `print.net.conf` bit 0: wired Ethernet is the active connection.
pub(crate) const NET_CONF_WIRED: u32 = 1 << 0;
/// The fixed `wifi_signal` a wired-only printer reports in place of a signal strength.
pub(crate) const WIRED_WIFI_SIGNAL: &str = "-90dBm";

/// Whether `print.net.conf` reports wired Ethernet as the active connection.
pub(crate) fn is_wired(net_conf: u32) -> bool {
    net_conf & NET_CONF_WIRED != 0
}

/// Whether a `wifi_signal` value is the wired-only sentinel.
pub(crate) fn is_wired_wifi_signal(wifi_signal: &str) -> bool {
    wifi_signal == WIRED_WIFI_SIGNAL
}

// ---------------------------------------------------------------------------
// Capability strings
// ---------------------------------------------------------------------------

/// `fun` bit 29: MQTT signature required — clear means Developer LAN Mode is on [REF-MQTT-ENV §3.2.1].
pub(crate) const FUN_MQTT_SIGNATURE_REQUIRED_BIT: u32 = 29;
/// `fun2` bit reporting the printer's own remote-dry support (`DeviceManager.cpp:4469`).
pub const FUN2_REMOTE_DRY_BIT: u32 = 5;
/// `flag3` bit 9, BambuStudio's `is_enable_ams_np`.
pub(crate) const FLAG3_AMS_NEW_PROTOCOL: u32 = 1 << 9;

// ---------------------------------------------------------------------------
// `xcam.cfg`
// ---------------------------------------------------------------------------

/// Bit positions within `xcam.cfg`, per BambuStudio `DeviceCore/DevPrintOptions.cpp:41-85`.
///
/// The four AI failure detectors sit on a stride-3 layout: an enable bit, then a two-bit
/// sensitivity field immediately *above* it. bambuddy places the sensitivity pair *below* the
/// enable bit instead (`bambu_mqtt.py:2636`, `decode_detector(5)`) — a pure phase difference.
/// BambuStudio is followed here; see `src/types/telemetry/CLAUDE.md` before "fixing" it back.
pub(crate) mod xcam_cfg {
    /// Spaghetti-detection enable bit; sensitivity in bits 8-9.
    pub(crate) const SPAGHETTI: u32 = 7;
    /// Purge-chute-pileup enable bit; sensitivity in bits 11-12.
    pub(crate) const PURGE_CHUTE_PILEUP: u32 = 10;
    /// Nozzle-clumping enable bit; sensitivity in bits 14-15.
    pub(crate) const NOZZLE_CLUMPING: u32 = 13;
    /// Air-printing enable bit; sensitivity in bits 17-18.
    pub(crate) const AIR_PRINTING: u32 = 16;
    /// Buildplate-alignment detection enable bit (no sensitivity field).
    pub(crate) const BUILDPLATE_ALIGN: u32 = 20;
    /// Foreign-object-detection check enable bit (no sensitivity field).
    pub(crate) const FOD_CHECK: u32 = 21;
    /// Displacement-detection enable bit (no sensitivity field).
    pub(crate) const DISPLACEMENT: u32 = 22;

    /// Offset from a detector's enable bit to the low bit of its sensitivity field.
    pub(crate) const SENSITIVITY_OFFSET: u32 = 1;
    /// Width of a detector's sensitivity field, in bits.
    pub(crate) const SENSITIVITY_WIDTH: u32 = 2;
}

// ---------------------------------------------------------------------------
// Hex fields
// ---------------------------------------------------------------------------

/// Strips surrounding whitespace and an optional `0x`/`0X` prefix.
fn strip_hex_prefix(s: &str) -> &str {
    let s = s.trim();
    s.strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s)
}

/// Parses a fixed-width hex field (`stat`, `print_error`, an AMS `info`) into a `u64`.
///
/// Whitespace and an optional `0x`/`0X` prefix are ignored. `None` for anything else that isn't
/// hex, or for more than 16 digits — use [`hex_bit`] for capability strings, which can be longer.
pub(crate) fn hex_u64(s: &str) -> Option<u64> {
    u64::from_str_radix(strip_hex_prefix(s), 16).ok()
}

/// Reads one bit of an unbounded-length hex capability string (`fun`, `fun2`), LSB-first from the right.
///
/// `fun2` "may have infinite length" per BambuStudio's own comment (`DeviceManager.cpp:4464`),
/// which is why this walks hex digits from the right instead of parsing into an integer — a
/// string longer than 16 digits would fail that parse outright and report every capability as
/// absent.
///
/// Mirrors `DevUtil::get_flag_bits_no_border` (`DevUtil.cpp:27-90`): whitespace, a `0x`/`0X`
/// prefix and any non-hex characters are ignored, and an index past the end of the string reads
/// `false` rather than failing. Returns `None` only when no hex digits remain after filtering.
#[must_use]
pub fn hex_bit(hex: &str, bit: u32) -> Option<bool> {
    let digits = strip_hex_prefix(hex).as_bytes();
    let nibble_from_right = (bit / 4) as usize;
    let mut seen = 0usize;
    let mut any = false;
    for &byte in digits.iter().rev() {
        let Some(value) = (byte as char).to_digit(16) else {
            continue;
        };
        any = true;
        if seen == nibble_from_right {
            return Some((value >> (bit % 4)) & 1 == 1);
        }
        seen += 1;
    }
    // Ran off the left end of the string: those bits are zero, not unknown — but a string with
    // no hex digits at all never told us anything.
    any.then_some(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_u64_accepts_prefix_case_and_whitespace() {
        assert_eq!(hex_u64("0x00800000"), Some(0x0080_0000));
        assert_eq!(hex_u64(" 0X20000000"), Some(0x2000_0000));
        assert_eq!(hex_u64("ff"), Some(0xff));
        assert_eq!(hex_u64("zzzz"), None);
        assert_eq!(hex_u64(""), None);
    }

    #[test]
    fn test_hex_bit_accepts_uppercase_prefix_and_whitespace() {
        assert_eq!(hex_bit(" 0X20000000", 29), Some(true));
        assert_eq!(hex_bit("0x20", 5), Some(true));
    }

    #[test]
    fn test_homing_helpers() {
        assert!(is_axis_homed(HOME_FLAG_Y, Axis::Y));
        assert!(!is_axis_homed(HOME_FLAG_Y, Axis::X));
        assert!(is_all_axes_homed(HOME_FLAG_XYZ | HOME_FLAG_POWER_220V));
        assert!(!is_all_axes_homed(HOME_FLAG_X | HOME_FLAG_Z));
    }
}
