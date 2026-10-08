//! # Quirk Context
//!
//! The wire-derived inputs a [`ModelQuirks`](super::ModelQuirks) method may consult.
//!
//! Some capabilities are a property of the model alone — a printer either has a second
//! auxiliary fan or it does not, and nothing it reports changes that. Others the machine answers
//! for itself, either through a capability bitfield or through its firmware version, and for
//! those a per-model table is a claim about every unit of that model while the report is the
//! machine in front of you speaking.
//!
//! Quirks in the second category take a `QuirkContext` rather than composing the report at the
//! call site. That is deliberate and load-bearing: when the composition lives outside the quirk,
//! two callers can reach different answers to the same question, which is exactly what happened
//! between `ModelQuirks::ams_remote_drying_support` and
//! `PrinterClient::supports_ams_remote_drying` before #240. Requiring the context makes the
//! stale-answer call impossible to write rather than merely discouraged.
//!
//! Every field is `Option` because every one of them can be genuinely absent, and absent is not
//! the same as `false`. The P1 and A1 families send no `fun2` at all
//! (`reference/03_mqtt_telemetry.md`), and firmware version needs a `get_version` round trip
//! that a caller may never have made. A quirk reading `None` should fall back to its model
//! default, not treat it as a denial.
//!
//! Build one from a client with [`PrinterClient::quirk_context`](crate::client::PrinterClient::quirk_context),
//! or reach for [`PrinterClient::capabilities`](crate::client::PrinterClient::capabilities), which
//! supplies it for you.
//!
//! A field is added when a quirk reads it, not ahead of need.

/// Wire-derived inputs a quirk may consult, all optional.
///
/// See the [module docs](self) for why these are passed in rather than composed at the call
/// site.
#[derive(Debug, Clone, Copy, Default)]
pub struct QuirkContext<'a> {
    /// The `fun2` capability bitfield, if the printer reported one.
    ///
    /// Absent on the P1 and A1 families entirely, so a quirk that prefers a `fun2` bit is inert
    /// on those models and must still carry a sound model default. Single-source: only
    /// BambuStudio reads this field.
    pub fun2: Option<&'a str>,

    /// The printer's OTA firmware version (`module[name="ota"].sw_ver`, see [`OTA_MODULE_NAME`](crate::types::version::OTA_MODULE_NAME), e.g. `"01.09.00.00"`),
    /// if a `get_version` response has been seen.
    ///
    /// Several capabilities ship in a specific firmware release rather than being inherent to
    /// the model — remote AMS drying is version-gated on H2D, H2D Pro, H2S, H2C, P2S and X2D for
    /// exactly this reason.
    pub firmware: Option<&'a str>,

    /// The `fun` capability bitfield, if the printer reported one.
    ///
    /// Absent on the P1 and A1 families, like `fun2`. Where present it carries per-setting
    /// support bits for several `print_option` settings, and BambuStudio lets it override the
    /// matching `home_flag` bit.
    pub fun: Option<&'a str>,

    /// The `home_flag` bitfield of a full status report, if one was observed on the current connection.
    ///
    /// The capability field every family sends, so it is the only reported support signal on P1
    /// and A1. Taken from a full report only, since H2D heartbeat frames carry a partial
    /// `home_flag`, and from the current connection only, since what the printer supports can
    /// change across a reboot.
    pub home_flag: Option<u32>,
}

impl<'a> QuirkContext<'a> {
    /// An empty context — every input absent, so every quirk falls back to its model default.
    ///
    /// Use when no telemetry has been seen, or to ask what a model claims about itself before
    /// any report has arrived.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Sets the `fun2` capability bitfield.
    #[must_use]
    pub fn with_fun2(mut self, fun2: Option<&'a str>) -> Self {
        self.fun2 = fun2;
        self
    }

    /// Sets the OTA firmware version.
    #[must_use]
    pub fn with_firmware(mut self, firmware: Option<&'a str>) -> Self {
        self.firmware = firmware;
        self
    }

    /// Sets the `fun` capability bitfield.
    #[must_use]
    pub fn with_fun(mut self, fun: Option<&'a str>) -> Self {
        self.fun = fun;
        self
    }

    /// Sets the `home_flag` bitfield.
    #[must_use]
    pub fn with_home_flag(mut self, home_flag: Option<u32>) -> Self {
        self.home_flag = home_flag;
        self
    }
}

/// Compares two Bambu firmware version strings, returning true if `have` is at least `want`.
///
/// Versions are dotted numeric quads (`"01.09.00.00"`). Compared component-wise as integers
/// rather than lexicographically: upstream's zero-padded strings happen to sort correctly as
/// text, but a single unpadded component (`"1.9.0.0"`) would silently compare wrong, and nothing
/// guarantees the padding.
///
/// Missing trailing components read as `0`, so `"01.09"` and `"01.09.00.00"` are equal. A
/// component that isn't a number makes the whole comparison `false` — an unparseable version
/// cannot be shown to meet a minimum, and claiming a capability on a string we failed to read is
/// the wrong direction to fail.
#[must_use]
pub fn firmware_at_least(have: &str, want: &str) -> bool {
    let mut have_parts = have.trim().split('.');
    let mut want_parts = want.trim().split('.');
    // `None` for an unparseable component; a missing or empty one reads as 0.
    fn component(part: Option<&str>) -> Option<u32> {
        match part {
            None | Some("") => Some(0),
            Some(part) => part.trim().parse().ok(),
        }
    }
    for _ in 0..4 {
        let (Some(h), Some(w)) = (component(have_parts.next()), component(want_parts.next()))
        else {
            return false;
        };
        if h != w {
            return h > w;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_firmware_at_least_compares_componentwise() {
        assert!(firmware_at_least("01.09.00.00", "01.09.00.00"));
        assert!(firmware_at_least("01.09.00.01", "01.09.00.00"));
        assert!(firmware_at_least("01.10.00.00", "01.09.00.00"));
        assert!(!firmware_at_least("01.08.99.99", "01.09.00.00"));

        // The case lexicographic comparison gets wrong: 10 > 9 numerically, but "10" < "9"
        // as text once the padding is gone.
        assert!(firmware_at_least("1.10.0.0", "1.9.0.0"));
        assert!(!firmware_at_least("1.9.0.0", "1.10.0.0"));

        // Missing trailing components are zero.
        assert!(firmware_at_least("01.09", "01.09.00.00"));
        assert!(firmware_at_least("01.09.00.00", "01.09"));
        assert!(!firmware_at_least("01.08", "01.09"));
    }

    #[test]
    fn test_firmware_at_least_rejects_unparseable() {
        // An unreadable version cannot be shown to meet a minimum. Failing false keeps a
        // capability claim off a printer whose version we could not parse.
        assert!(!firmware_at_least("", "01.09.00.00"));
        assert!(!firmware_at_least("garbage", "01.09.00.00"));
        assert!(!firmware_at_least("01.09.00.00-beta", "01.09.00.00"));
        assert!(!firmware_at_least("01.09.00.00", "not-a-version"));
    }

    #[test]
    fn test_context_builders() {
        let ctx = QuirkContext::empty()
            .with_fun2(Some("20"))
            .with_firmware(Some("01.09.00.00"));
        assert_eq!(ctx.fun2, Some("20"));
        assert_eq!(ctx.firmware, Some("01.09.00.00"));

        let empty = QuirkContext::empty();
        assert_eq!(empty.fun2, None);
        assert_eq!(empty.firmware, None);
    }
}
