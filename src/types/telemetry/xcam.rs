//! AI failure-detection and print-option settings (`print.xcam`).
//!
//! Two protocol generations share this one wire object, and BambuStudio branches between them
//! (`DeviceCore/DevPrintOptions.cpp:39-85`). New-gen firmware sends `xcam.cfg`, a packed integer
//! bitmask carrying every detector's enable bit and sensitivity level. Old-gen firmware sends
//! discrete boolean keys instead. Both are modeled here; the accessors prefer `cfg` when present,
//! mirroring BambuStudio's own precedence.
//!
//! `xcam.cfg` is **not** the top-level `print.cfg` hex string ([`super::report::PrinterTelemetry`]) —
//! same key name, different container, different type. Do not conflate them.

#[cfg(not(feature = "std"))]
use alloc::collections::BTreeMap;
#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(feature = "std")]
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Bit positions within `xcam.cfg`, per BambuStudio `DeviceCore/DevPrintOptions.cpp:41-85`.
///
/// The four AI failure detectors sit on a stride-3 layout: an enable bit, then a two-bit
/// sensitivity field immediately *above* it. bambuddy places the sensitivity pair *below* the
/// enable bit instead (`bambu_mqtt.py:2636`, `decode_detector(5)`) — a pure phase difference.
/// BambuStudio is followed here; see this module's `CLAUDE.md` note before "fixing" it back.
pub(crate) mod cfg_bits {
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

/// Sensitivity level attached to an AI failure detector.
///
/// Applies only to the four camera-based failure detectors (spaghetti, purge-chute pileup,
/// nozzle clumping, air printing). Unrelated to skip-objects or to `allow_skip_parts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum XcamSensitivity {
    /// Least eager to halt the print.
    Low,
    /// Firmware default on every capture observed so far.
    Medium,
    /// Most eager to halt the print.
    High,
}

impl XcamSensitivity {
    /// Maps a raw two-bit sensitivity field to a level, or `None` for the unassigned value `3`.
    ///
    /// BambuStudio leaves its previous value in place on `3` rather than choosing a level, so
    /// there is no documented meaning to map it to.
    pub(crate) fn from_bits(bits: u32) -> Option<Self> {
        match bits {
            0 => Some(Self::Low),
            1 => Some(Self::Medium),
            2 => Some(Self::High),
            _ => None,
        }
    }

    /// Returns the wire spelling BambuStudio uses for this level (`"low"`/`"medium"`/`"high"`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// One AI failure detector's decoded state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XcamDetector {
    /// Whether the firmware is running this detector.
    pub enabled: bool,
    /// How eagerly it halts the print, or `None` if the field holds the unassigned value `3`.
    pub sensitivity: Option<XcamSensitivity>,
}

/// AI detection and print-option settings, nested as `print.xcam` on the wire.
///
/// Every field is `Option` because which keys arrive depends on both firmware generation and
/// model, and because `xcam` appears to be pushall-only — an incremental `msg: 1` frame carries
/// none of it. Use [`merge_from`](Self::merge_from) rather than replacing a cached copy wholesale.
///
/// Unmodeled keys survive in [`extra`](Self::extra): this wire object is still largely uncharted
/// and model-dependent, so round-tripping a report must not silently drop what it carries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct XcamTelemetry {
    /// New-gen packed detector bitmask. Decode via the detector accessors, not by hand.
    ///
    /// Its presence is also what BambuStudio uses to decide the printer supports AI monitoring
    /// at all — see [`supports_ai_monitoring`](Self::supports_ai_monitoring).
    pub cfg: Option<u32>,

    /// Mid-generation AI-monitoring master switch, superseded by `cfg`.
    pub printing_monitor: Option<bool>,

    /// Oldest-generation AI-monitoring master switch, superseded by `printing_monitor`.
    pub spaghetti_detector: Option<bool>,

    /// Whether a detected failure halts the print rather than only warning.
    pub print_halt: Option<bool>,

    /// AI-monitoring sensitivity as a bare string (`"low"`/`"medium"`/`"high"`).
    ///
    /// Old-gen only, and bambuddy reports it as reliably stale (`bambu_mqtt.py:2723`, "it's always
    /// stale"). Prefer the per-detector sensitivity off `cfg` whenever `cfg` is present.
    pub halt_print_sensitivity: Option<String>,

    /// Whether first-layer inspection is enabled.
    pub first_layer_inspector: Option<bool>,

    /// Whether buildplate-marker detection is enabled.
    pub buildplate_marker_detector: Option<bool>,

    /// Per-job or runtime flag whose exact semantics are **not settled** — do not gate anything
    /// on it.
    ///
    /// It is not a model-capability flag (Bambu documents every current model as supporting
    /// skip-objects, yet this reads `false` in all twelve upstream fixtures carrying it) and not a
    /// user setting (no such setting exists in Bambu Studio or on the printer). Gating
    /// [`crate::client::PrinterClient::skip_objects`] on it would break skip-objects outright on
    /// hardware that supports the feature.
    pub allow_skip_parts: Option<bool>,

    /// Every `xcam` key this struct does not model, preserved verbatim.
    ///
    /// `auto_recovery_step_loss` and `filament_tangle_detect` land here deliberately: bambuddy
    /// reads them out of `xcam` (`bambu_mqtt.py:2792-2795`, itself commented "tracked locally
    /// only"), but BambuStudio sources both from `home_flag` instead (bits 4 and 20), and no
    /// capture shows either inside `xcam`. Same for `ipcam_record`/`timelapse`, which bambino
    /// models under [`super::diagnostics::IpcamTelemetry`] from `print.ipcam`.
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl XcamTelemetry {
    /// Returns whether this printer supports on-device AI failure monitoring.
    ///
    /// Presence of `cfg` is the signal, matching BambuStudio's `is_support_detect` assignment.
    pub fn supports_ai_monitoring(&self) -> bool {
        self.cfg.is_some()
    }

    /// Returns whether AI print monitoring is enabled, across all three protocol generations.
    ///
    /// Checks `cfg`'s spaghetti-detection bit first, then `printing_monitor`, then
    /// `spaghetti_detector` — the same precedence BambuStudio applies. `None` means no
    /// generation's key was present.
    pub fn ai_monitoring_enabled(&self) -> Option<bool> {
        if let Some(detector) = self.spaghetti_detection() {
            return Some(detector.enabled);
        }
        self.printing_monitor.or(self.spaghetti_detector)
    }

    /// Returns spaghetti-detection state decoded from `cfg`, or `None` when `cfg` is absent.
    pub fn spaghetti_detection(&self) -> Option<XcamDetector> {
        self.detector(cfg_bits::SPAGHETTI)
    }

    /// Returns purge-chute-pileup detection state decoded from `cfg`, or `None` when `cfg` is absent.
    pub fn purge_chute_pileup_detection(&self) -> Option<XcamDetector> {
        self.detector(cfg_bits::PURGE_CHUTE_PILEUP)
    }

    /// Returns nozzle-clumping detection state decoded from `cfg`, or `None` when `cfg` is absent.
    pub fn nozzle_clumping_detection(&self) -> Option<XcamDetector> {
        self.detector(cfg_bits::NOZZLE_CLUMPING)
    }

    /// Returns air-printing detection state decoded from `cfg`, or `None` when `cfg` is absent.
    pub fn air_printing_detection(&self) -> Option<XcamDetector> {
        self.detector(cfg_bits::AIR_PRINTING)
    }

    /// Returns whether buildplate-alignment detection is enabled, or `None` when `cfg` is absent.
    pub fn buildplate_align_detection(&self) -> Option<bool> {
        self.cfg_bit(cfg_bits::BUILDPLATE_ALIGN)
    }

    /// Returns whether the foreign-object-detection check is enabled, or `None` when `cfg` is absent.
    pub fn fod_check(&self) -> Option<bool> {
        self.cfg_bit(cfg_bits::FOD_CHECK)
    }

    /// Returns whether displacement detection is enabled, or `None` when `cfg` is absent.
    pub fn displacement_detection(&self) -> Option<bool> {
        self.cfg_bit(cfg_bits::DISPLACEMENT)
    }

    /// Decodes one stride-3 detector triple (enable bit plus the sensitivity pair above it).
    fn detector(&self, enable_bit: u32) -> Option<XcamDetector> {
        let cfg = self.cfg?;
        let sensitivity_bits = (cfg >> (enable_bit + cfg_bits::SENSITIVITY_OFFSET))
            & ((1 << cfg_bits::SENSITIVITY_WIDTH) - 1);
        Some(XcamDetector {
            enabled: (cfg >> enable_bit) & 1 != 0,
            sensitivity: XcamSensitivity::from_bits(sensitivity_bits),
        })
    }

    /// Reads a single standalone `cfg` bit that carries no sensitivity field.
    fn cfg_bit(&self, bit: u32) -> Option<bool> {
        self.cfg.map(|cfg| (cfg >> bit) & 1 != 0)
    }

    /// Merges a freshly-parsed `XcamTelemetry` into `self` field-by-field.
    ///
    /// Mirrors `IpcamTelemetry::merge_from` and exists for the same reason:
    /// a frame that carries only part of the object must not blank the rest of a cached copy.
    /// Present fields overwrite; absent ones leave the cached value alone. `extra` merges per key
    /// rather than being replaced, so an unmodeled key seen once survives later partial frames.
    pub fn merge_from(&mut self, incoming: &XcamTelemetry) {
        if incoming.cfg.is_some() {
            self.cfg = incoming.cfg;
        }
        if incoming.printing_monitor.is_some() {
            self.printing_monitor = incoming.printing_monitor;
        }
        if incoming.spaghetti_detector.is_some() {
            self.spaghetti_detector = incoming.spaghetti_detector;
        }
        if incoming.print_halt.is_some() {
            self.print_halt = incoming.print_halt;
        }
        if incoming.halt_print_sensitivity.is_some() {
            self.halt_print_sensitivity = incoming.halt_print_sensitivity.clone();
        }
        if incoming.first_layer_inspector.is_some() {
            self.first_layer_inspector = incoming.first_layer_inspector;
        }
        if incoming.buildplate_marker_detector.is_some() {
            self.buildplate_marker_detector = incoming.buildplate_marker_detector;
        }
        if incoming.allow_skip_parts.is_some() {
            self.allow_skip_parts = incoming.allow_skip_parts;
        }
        for (key, value) in &incoming.extra {
            self.extra.insert(key.clone(), value.clone());
        }
    }
}
