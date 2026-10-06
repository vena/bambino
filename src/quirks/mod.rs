//! # Model-Specific Quirks
//!
//! Bambu Lab printers vary in hardware capabilities — door sensors, chamber heaters,
//! fan step resolution, FTPS TLS requirements, camera protocols, and more. Rather than
//! scattering `match model { ... }` blocks everywhere, [`ModelQuirks`] captures all
//! model-specific behavior in one place. Call [`PrinterModel::quirks()`] to get any model's row.
//!
//! The per-model rows live in the [`models`] submodule, as data: one `const` per model. This
//! module also provides shared helpers like [`fan_step_to_percentage()`] and
//! [`FanSpeedDebouncer`] for dealing with the low-resolution PWM fan telemetry common across most
//! models.

pub mod context;
mod gcode;
pub mod models;

pub use context::{QuirkContext, firmware_at_least};

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::String;

use crate::camera::CameraProtocol;
use crate::error::Error;
use crate::models::PrinterModel;

/// Reads the printer's own remote-dry answer out of a [`QuirkContext`]'s `fun2` string.
///
/// `None` means the printer never reported `fun2`, or reported one carrying no hex digits —
/// "it didn't say", which leaves the model's [`DryRule`] to decide. A `Some` is the printer's
/// answer and every rule must honor it; this helper exists so none of them can read the bit
/// differently.
///
/// **`None` is the normal case on P1 and A1**, which send no `fun2` at all
/// (`reference/03_mqtt_telemetry.md`), so the model rules are what actually govern on those
/// families — not a rarely-taken fallback.
pub(crate) fn reported_remote_dry(ctx: &QuirkContext) -> Option<bool> {
    ctx.fun2.and_then(|hex| {
        crate::types::telemetry::fun2_bit(hex, crate::types::telemetry::FUN2_REMOTE_DRY_BIT)
    })
}

/// How a capability answer was reached — the printer's own report, an inference, or a default.
///
/// A plain `bool` collapses "this X1C reported the bit clear" and "no telemetry has arrived, so
/// yes was assumed" into the same value, though they warrant opposite handling: the first is
/// settled, the second means "ask again once connected". [`is_supported`](Self::is_supported)
/// collapses back to that `bool` for callers that don't need the distinction.
///
/// Only capabilities that resolve a reported value against model rules against a default carry
/// this type. Static model facts (build volume, camera protocol, …) have no provenance question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    /// The printer said so itself, e.g. through a `fun2` capability bit.
    Reported(bool),
    /// Derived from what is known about this printer without it saying so: its firmware version
    /// against a documented threshold, or a documented rule for its model.
    Inferred(bool),
    /// Nothing about this printer settles it, so this is the capability's default.
    Assumed(bool),
}

impl Support {
    /// The answer with its provenance discarded.
    #[must_use]
    pub fn is_supported(self) -> bool {
        match self {
            Self::Reported(v) | Self::Inferred(v) | Self::Assumed(v) => v,
        }
    }
}

/// Compares the context's firmware against `min_firmware`, or returns `unknown` when no version
/// has been read.
pub(crate) fn firmware_gate(ctx: &QuirkContext, min_firmware: &str, unknown: Support) -> Support {
    ctx.firmware.map_or(unknown, |have| {
        Support::Inferred(firmware_at_least(have, min_firmware))
    })
}

/// A model's rule for AMS drying, when the printer doesn't report it itself.
///
/// One rule answers both [`ModelQuirks::ams_remote_drying_support`] and
/// [`ModelQuirks::ams_drying_while_printing_support`]: every model's vendor sources state the
/// two together (same release, same "not supported yet" list), so a per-capability rule would
/// only be a second place to get one model wrong.
///
/// Both resolvers honor a reported `fun2` bit 5 first — see
/// [`ModelQuirks::ams_remote_drying_support`] for which direction counts for which capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DryRule {
    /// No vendor source states either way (X1E, an unrecognized model): idle remote drying is
    /// assumed allowed — the printer answers `result: "fail"` if it can't, matching bambuddy's
    /// "all other models ... are allowed" — and drying while printing is assumed denied.
    Unstated,
    /// The vendor names the model unsupported: the drying guide's "not supported yet" lists,
    /// or a release history that puts drying on the printer's screen only.
    Never,
    /// The model's earliest published release already has both capabilities.
    Always,
    /// Both capabilities shipped in release `min`.
    ///
    /// **An unread firmware version does not deny idle remote drying.** `None` means "nobody has
    /// asked this printer yet", never "the printer refused to say": a non-answering printer makes
    /// [`get_version`](crate::client::PrinterClient::get_version) return `Err(Error::Timeout)`.
    /// Denying on `None` would make an H2S's drying support depend on whether the caller happened
    /// to call `get_version()` first — invisible, order-dependent, and wrong in the direction
    /// that hides a capability the printer has. bambuddy's equivalent gate reads
    /// `bool(firmware and firmware >= ...)` and so denies on `None`, but that guard is required
    /// by Python — `None >= "01.09.00.00"` raises `TypeError` — on a background scheduler where
    /// skipping a printer is free and retried. Neither reason transfers to a library whose caller
    /// asked once, so the thresholds are ported and the `None` handling is not.
    ///
    /// **Drying while printing resolves `None` the other way, to deny**, and deliberately: the
    /// vendor's list for it is a closed allowlist, and an unsupported printer refuses mid-print
    /// with `dry_sf_reason` `0` anyway, so assuming no hides nothing a caller could have used.
    /// bambuddy's `supports_drying_while_printing` agrees. Don't align the two.
    ///
    /// `in_first_release` marks a `min` that is the model's earliest published release: then
    /// every unit has it, and an unread version is inferred supported for both capabilities.
    Firmware {
        min: &'static str,
        in_first_release: bool,
    },
}

impl DryRule {
    fn remote(self, ctx: &QuirkContext) -> Support {
        let rule = match self {
            Self::Unstated => Support::Assumed(true),
            Self::Never => Support::Inferred(false),
            Self::Always => Support::Inferred(true),
            Self::Firmware {
                min,
                in_first_release,
            } => firmware_gate(ctx, min, first_release_or(in_first_release, true)),
        };
        reported_remote_dry(ctx).map_or(rule, Support::Reported)
    }

    fn while_printing(self, ctx: &QuirkContext) -> Support {
        // No `fun2` bit reports this capability. A reported *clear* remote-dry bit still refuses
        // it, because drying while printing is strictly narrower than idle remote drying; a *set*
        // bit says nothing about the concurrent case and is not evidence for it.
        if reported_remote_dry(ctx) == Some(false) {
            return Support::Reported(false);
        }
        match self {
            Self::Unstated => Support::Assumed(false),
            Self::Never => Support::Inferred(false),
            Self::Always => Support::Inferred(true),
            Self::Firmware {
                min,
                in_first_release,
            } => firmware_gate(ctx, min, first_release_or(in_first_release, false)),
        }
    }
}

/// `Inferred(true)` when the gating release is the model's first, else `Assumed(default)`.
fn first_release_or(in_first_release: bool, default: bool) -> Support {
    if in_first_release {
        Support::Inferred(true)
    } else {
        Support::Assumed(default)
    }
}

/// Where a model reports its front door state, if it has a door sensor at all [REF-NET-DOOR].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorSensor {
    /// No electronic door sensor, so no door state is ever reported.
    None,
    /// `home_flag` bit 23 (X1 series) — see
    /// [`PrinterTelemetry::is_door_open_from_home_flag`](crate::types::PrinterTelemetry::is_door_open_from_home_flag).
    HomeFlag,
    /// `stat` (H2, P2S and X2D series) — see
    /// [`PrinterTelemetry::is_door_open_from_stat`](crate::types::PrinterTelemetry::is_door_open_from_stat).
    Stat,
}

/// How a model's hotends are arranged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NozzleLayout {
    /// One hotend.
    Single,
    /// Two hotends on independent carriages (IDEX).
    Dual,
    /// Hotends mounted from a swappable tool-changer rack (H2C: 1 fixed + 6 rack hotends).
    ///
    /// A rack model addresses nozzles by *physical ID* rather than by extruder index, and the two
    /// namespaces overlap in a way that makes an untranslated value silently wrong rather than
    /// obviously wrong — see [`crate::mqtt::resolve_rack_nozzle_mapping`].
    Rack {
        /// Every hotend the machine can hold, the fixed one included.
        nozzles: u8,
    },
}

impl NozzleLayout {
    /// Returns the number of physical hotends, rack slots included.
    #[must_use]
    pub fn nozzle_count(self) -> u8 {
        match self {
            Self::Single => 1,
            Self::Dual => 2,
            Self::Rack { nozzles } => nozzles,
        }
    }
}

/// A model's maximum safe travel per axis, in millimeters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildVolume {
    /// X-axis travel ceiling.
    pub x: f32,
    /// Y-axis travel ceiling.
    pub y: f32,
    /// Z-axis travel ceiling.
    pub z: f32,
}

impl BuildVolume {
    /// A cube with every axis at `side`.
    pub(crate) const fn cube(side: f32) -> Self {
        Self {
            x: side,
            y: side,
            z: side,
        }
    }
}

/// A model's heated-bed ceiling, flat or dependent on mains voltage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BedMax {
    Flat(u16),
    /// Different ceilings per mains region (X1, X1C). With the region unknown, the lower one.
    Voltage {
        v220: u16,
        v110: u16,
    },
}

/// The physical-safety facts every model row must state; `ModelQuirks::new` takes no defaults
/// for these.
pub(crate) struct SafetyLimits {
    pub volume: BuildVolume,
    pub nozzle_temp_max: u16,
    pub bed_temp_max: BedMax,
    /// `None` when the model has no active chamber heater.
    pub chamber_heater_temp_max: Option<u16>,
    /// Whether the build plate moves along Z (CoreXY) rather than Y (bed-slinger).
    pub bed_on_z: bool,
}

/// One printer model's hardware variations and transport exceptions, as data.
///
/// Get one from [`PrinterModel::quirks()`]. Every model is one `const` row in
/// [`models`], built from `ModelQuirks::new` (which demands the safety limits,
/// camera protocol, AMS layout and drying rule) plus overrides of the defaulted fields. Fields
/// are private: a row is a claim about real hardware, so only this crate writes one.
///
/// Method names follow one convention: `has_*` for physical hardware present on the machine,
/// `supports_*` for a firmware feature, and `uses_*`/`requires_*` for protocol behavior the client
/// must adapt to.
#[derive(Debug, Clone, Copy)]
pub struct ModelQuirks {
    volume: BuildVolume,
    nozzle_temp_max: u16,
    bed_temp_max: BedMax,
    chamber_heater_temp_max: Option<u16>,
    bed_on_z: bool,
    camera: CameraProtocol,
    ams_pool: crate::ams::AmsPoolComposition,
    dry: DryRule,
    plaintext_ftps_data_channel: bool,
    ftps_tls_1_2: bool,
    door: DoorSensor,
    chamber_temperature_sensor: bool,
    nozzles: NozzleLayout,
    heatbed_thermal_calibration: bool,
    wallclock_rtsp_timestamps: bool,
    auxiliary_left_fan: bool,
    auxiliary_left2_fan: bool,
    chamber_exhaust_fan: bool,
    airduct_mode: bool,
    prompt_sound: bool,
    buzzer: bool,
}

impl ModelQuirks {
    /// A row with the required facts set and every other field at its most common value.
    ///
    /// Defaults: encrypted FTPS data channel, no TLS 1.2 cap, no door sensor, no chamber
    /// temperature sensor, one nozzle, no heatbed thermal calibration, RTP timestamps, a primary
    /// left auxiliary fan and nothing else optional.
    pub(crate) const fn new(
        limits: SafetyLimits,
        camera: CameraProtocol,
        ams_pool: crate::ams::AmsPoolComposition,
        dry: DryRule,
    ) -> Self {
        Self {
            volume: limits.volume,
            nozzle_temp_max: limits.nozzle_temp_max,
            bed_temp_max: limits.bed_temp_max,
            chamber_heater_temp_max: limits.chamber_heater_temp_max,
            bed_on_z: limits.bed_on_z,
            camera,
            ams_pool,
            dry,
            plaintext_ftps_data_channel: false,
            ftps_tls_1_2: false,
            door: DoorSensor::None,
            chamber_temperature_sensor: false,
            nozzles: NozzleLayout::Single,
            heatbed_thermal_calibration: false,
            wallclock_rtsp_timestamps: false,
            auxiliary_left_fan: true,
            auxiliary_left2_fan: false,
            chamber_exhaust_fan: false,
            airduct_mode: false,
            prompt_sound: false,
            buzzer: false,
        }
    }

    /// Returns true if this model requires a plaintext FTPS passive data channel (PROT C) due to board limitations [REF-FTPS-CONN].
    #[must_use]
    pub fn uses_plaintext_ftps_data_channel(&self) -> bool {
        self.plaintext_ftps_data_channel
    }

    /// Returns true if this model's FTPS server must be reached over TLS 1.2 or lower [REF-FTPS-CONN].
    ///
    /// This is a firmware bug workaround, not a real protocol ceiling. Both caps (P2S and X2D) are
    /// **confirmed by symptom with no confirmed mechanism** — each reporter saw the failure
    /// clear when the cap was applied, but neither root cause has been traced, and the X2D's
    /// original explanation has since been measured wrong. A cap costs nothing on a printer that
    /// never offers TLS 1.3, so both are kept. See the `P2S` and `X2D` rows in
    /// `src/quirks/models/` for per-model evidence.
    #[must_use]
    pub fn requires_ftps_tls_1_2(&self) -> bool {
        self.ftps_tls_1_2
    }

    /// Returns where this model reports its door state, or [`DoorSensor::None`] without a door sensor.
    ///
    /// Read the state itself with
    /// [`PrinterTelemetry::door_state`](crate::types::PrinterTelemetry::door_state).
    #[must_use]
    pub fn door_sensor(&self) -> DoorSensor {
        self.door
    }

    /// Returns the camera streaming protocol used by this model's hardware [REF-NET-PORTS].
    #[must_use]
    pub fn camera_protocol(&self) -> CameraProtocol {
        self.camera
    }

    /// Returns true if the model has a physical chamber temperature sensor [REF-THER-DECODE].
    ///
    /// False on open-frame and entry-level machines (A1, A1 Mini, A2L, P1P, P1S), whose reported
    /// chamber value is not a measurement.
    #[must_use]
    pub fn has_chamber_temperature_sensor(&self) -> bool {
        self.chamber_temperature_sensor
    }

    /// Returns how this model's hotends are arranged.
    #[must_use]
    pub fn nozzle_layout(&self) -> NozzleLayout {
        self.nozzles
    }

    /// Returns the number of physical hotends: `1` single, `2` IDEX, `7` on the H2C's rack (1 fixed + 6 interchangeable).
    #[must_use]
    pub fn physical_nozzle_count(&self) -> u8 {
        self.nozzles.nozzle_count()
    }

    /// Returns true if the model mounts its hotends from a swappable tool-changer rack (H2C) — see [`NozzleLayout::Rack`].
    #[must_use]
    pub fn has_nozzle_rack(&self) -> bool {
        matches!(self.nozzles, NozzleLayout::Rack { .. })
    }

    /// Returns this model's physical AMS unit pool structure.
    ///
    /// Whether standard AMS and AMS-HT units share one combined pool or draw from independent
    /// pools, each pool's unit-count ceiling, and whether an AMS Lite attaches alongside or
    /// instead of the shared pool. Confirmed against `MODEL_MATRIX.csv`'s "AMS Unit Limits" row.
    /// Size a per-unit UI from
    /// [`AmsPoolComposition::max_units`](crate::ams::AmsPoolComposition::max_units).
    #[must_use]
    pub fn ams_pool_composition(&self) -> crate::ams::AmsPoolComposition {
        self.ams_pool
    }

    /// Returns true if the model runs nozzle offset calibration — every model with more than one hotend.
    #[must_use]
    pub fn supports_nozzle_offset_calibration(&self) -> bool {
        self.physical_nozzle_count() > 1
    }

    /// Returns true if the model runs heatbed leveling and thermal profile calibration (`calibration` option bit 5).
    ///
    /// False on every model so far. Observed inert on a P1S: the firmware accepts the bit,
    /// acknowledges the command `"result": "success"`, and queues no stage for it
    /// [REF-MQTT-LIFECYCLE]. Since the wire reports success either way, a model is assumed not to
    /// support this until a capture shows a stage queued for it: guessing wrong toward
    /// "unsupported" costs a rejected command rather than a silently skipped calibration.
    #[must_use]
    pub fn supports_heatbed_thermal_calibration(&self) -> bool {
        self.heatbed_thermal_calibration
    }

    /// Returns the mask of `calibration` option bits this model actually executes [REF-MQTT-LIFECYCLE].
    ///
    /// Bits 1–3 (bed leveling, vibration compensation, motor noise) are supported everywhere
    /// observed. Bit 4 follows [`Self::supports_nozzle_offset_calibration`] and bit 5 follows
    /// [`Self::supports_heatbed_thermal_calibration`]. Bits 0 and 6 are internal/undocumented
    /// and never included.
    #[must_use]
    pub fn supported_calibration_mask(&self) -> u32 {
        let mut mask = 0b0000_1110;
        if self.supports_nozzle_offset_calibration() {
            mask |= 0b0001_0000;
        }
        if self.supports_heatbed_thermal_calibration() {
            mask |= 0b0010_0000;
        }
        mask
    }

    /// Returns true if the build plate moves along the Z-axis (CoreXY bed-on-Z platforms) [REF-MOTO-GCODE].
    #[must_use]
    pub fn is_bed_on_z(&self) -> bool {
        self.bed_on_z
    }

    /// Checks raw G-code against this model's limits: what `PrinterClient::send_gcode` enforces.
    ///
    /// Rejects, with [`Error::ModelMismatch`]:
    ///
    /// - axis-constrained `G28` (Z, X or Y) on a bed-on-Z model, which risks a nozzle-to-plate
    ///   collision; bed-slingers allow every homing variant [REF-MOTO-GCODE];
    /// - an `M104`/`M109` `S`/`R`/`B` above [`Self::nozzle_temp_max`];
    /// - an `M140`/`M190` `S`/`R` above [`Self::bed_temp_max`] for `mains_220v`;
    /// - any `M141`/`M191` on a model without an active chamber heater, and an `S`/`R` above
    ///   [`Self::chamber_heater_temp_max`] on one with a heater.
    ///
    /// A temperature argument that isn't a plain decimal (`S3e2`, `S0x1F`) is rejected with
    /// [`Error::InvalidArgument`] rather than interpreted. Unlike the typed setters, nothing is
    /// clamped: the G-code is either sent as written or refused.
    ///
    /// Every statement is checked independently, splitting on `\n` and a bare `\r` —
    /// multi-statement payloads are a documented, supported wire shape (see `GCodeRequest`).
    /// Comments and a leading `M117` message are skipped; `G28X`, `G 28 Z` and `G028 Z` are all
    /// recognized as `G28`, since the firmware's parser is undocumented and the scan resolves
    /// every ambiguity toward rejecting.
    ///
    /// Relative moves are **not** bounded — the printer reports no absolute position, so there is
    /// nothing to bound them against; `move_relative` caps a single move's distance instead.
    pub fn validate_gcode(&self, gcode: &str, mains_220v: Option<bool>) -> Result<(), Error> {
        let temps = gcode::TempLimits {
            nozzle_max: self.nozzle_temp_max(),
            bed_max: self.bed_temp_max(mains_220v),
            chamber_max: self.chamber_heater_temp_max(),
        };
        gcode::validate(gcode, self.is_bed_on_z(), Some(&temps))
    }

    /// Returns this model's maximum safe travel per axis.
    #[must_use]
    pub fn build_volume(&self) -> BuildVolume {
        self.volume
    }

    /// Generates a model-compliant safe relative Z-axis movement G-code command [REF-MOTO-GCODE].
    ///
    /// Returns an empty string if `distance` exceeds the model's Z travel.
    #[must_use]
    pub fn relative_z_move_gcode(&self, distance: f32, feedrate: u32) -> String {
        format_z_move_gcode(distance, feedrate, self.volume.z)
    }

    /// Generates a bounded relative X/Y-axis movement G-code command.
    ///
    /// The same single-command distance cap `relative_z_move_gcode` applies to Z (see
    /// `format_z_move_gcode` for why this isn't true position-aware crash prevention). Returns an
    /// empty string if `distance` is zero, non-finite, exceeds the axis's travel, or `axis` is
    /// neither `'X'` nor `'Y'`.
    #[must_use]
    pub fn relative_xy_move_gcode(&self, axis: char, distance: f32, feedrate: u32) -> String {
        let axis_max = match axis {
            'X' => self.volume.x,
            'Y' => self.volume.y,
            _ => return String::new(),
        };
        format_xy_move_gcode(axis, distance, feedrate, axis_max)
    }

    /// Returns true if the model's RTSP camera stream requires wallclock timestamps instead of embedded RTP clock ticks to avoid frame freezing [REF-CAM-RTSPS].
    #[must_use]
    pub fn requires_wallclock_rtsp_timestamps(&self) -> bool {
        self.wallclock_rtsp_timestamps
    }

    /// Returns true if the model has a second left-side auxiliary fan (port 10) [REF-CLIM-FANS].
    ///
    /// Wire-labeled "right" but confirmed a left-side fan — see `FanTarget::AuxiliaryLeft2`'s doc
    /// comment, issue #60.
    #[must_use]
    pub fn has_auxiliary_left2_fan(&self) -> bool {
        self.auxiliary_left2_fan
    }

    /// Returns true if the model has a primary left-side auxiliary fan (port 2) [REF-CLIM-FANS].
    ///
    /// Every model except A1, A1 Mini, A2L (open-frame bed-slingers lacking this fan), P1P
    /// (`MODEL_MATRIX.csv` lists it `Optional`, not guaranteed present) and the plain X1
    /// (BambuStudio's X1 profile sets `auxiliary_fan` to `0`).
    #[must_use]
    pub fn has_auxiliary_left_fan(&self) -> bool {
        self.auxiliary_left_fan
    }

    /// Returns true if the model has a chamber exhaust/filtration fan (port 3): H2S, H2D, H2D Pro, H2C, X2D [REF-CLIM-FANS].
    #[must_use]
    pub fn has_chamber_exhaust_fan(&self) -> bool {
        self.chamber_exhaust_fan
    }

    /// Returns true if the model switches climate modes with airduct dampers: H2S, H2D, H2D Pro, H2C, P2S, X2D [REF-CLIM-FANS].
    #[must_use]
    pub fn supports_airduct_mode(&self) -> bool {
        self.airduct_mode
    }

    /// Returns true if the model plays prompt sound notifications: A1, A1 Mini, A2L (per Bambu Studio profiles).
    #[must_use]
    pub fn supports_prompt_sound(&self) -> bool {
        self.prompt_sound
    }

    /// Returns true if the model has a fire alarm buzzer module: H2S, H2D, H2D Pro, H2C (per pybambu).
    #[must_use]
    pub fn has_buzzer(&self) -> bool {
        self.buzzer
    }

    /// Returns whether `ams_filament_drying` sent over MQTT is honored by this printer, with its provenance.
    ///
    /// "Honored" rather than "accepted": an unsupported printer acks `result: success` and
    /// silently discards the command.
    ///
    /// Resolved in two stages. First, `ctx.fun2` bit 5 — the printer's own answer
    /// (`DeviceManager.cpp:4469`) — wins where it is present, in both directions, since a
    /// per-model rule is a claim about every unit of that model while `fun2` is the machine in
    /// front of you speaking. Second, when `fun2` is absent, the model's rule decides.
    ///
    /// **The second stage is the one that usually runs.** Only BambuStudio reads `fun2` at all,
    /// and the P1 and A1 families send no `fun2` (`reference/03_mqtt_telemetry.md`), so on a
    /// large share of real hardware the reported bit never appears.
    ///
    /// Model rules, sourced from Bambu Lab's per-model firmware release histories and its
    /// *Filament drying guide for AMS 2 Pro and AMS HT* wiki page (thresholds and rejected values
    /// tabulated in `reference/05_materials_ams.md` §5.4). BambuStudio has no model rule of its
    /// own — its `is_support_remote_dry` is a bare `false` initializer that only `fun2` ever sets:
    ///
    /// * **A1 / A1 Mini — never.** Not a hardware limit: these models take AMS 2 Pro and AMS-HT
    ///   units from the shared pool. No known firmware path exposes a remote-dry command; the
    ///   drying guide lists them as "not supported yet", and bambuddy lists them in
    ///   `_DRYING_UNSUPPORTED_MODELS`. The gate is about the command channel, not the attachable
    ///   hardware.
    /// * **P1P / P1S — never.** The AMS can dry, but only from the printer's own screen: P1
    ///   `01.08.00.00` (2025-04-29) says drying starts "from the printer's screen", no later P1
    ///   release adds remote drying, and the drying guide names both as unsupported. Bambu's P1
    ///   manual agrees ("P1S connected AMS drying functions may only be controlled from the P1S
    ///   screen"), bambuddy lists them in `_DRYING_SCREEN_ONLY_MODELS` citing its #2533, and this
    ///   crate's own drying command was tested against a P1S directly.
    /// * **X1, X1C — never.** The drying guide names the X1C alongside P1 and A1 as "not supported
    ///   yet"; X1 `01.09.00.00` (2025-04-29) carries the same screen-only sentence as P1
    ///   `01.08.00.00`, and no X1/X1C release through `01.12.00.00` mentions remote drying.
    /// * **H2D, H2D Pro, H2S, H2C, P2S, X2D — firmware-gated.** The capability shipped in a
    ///   specific release; see each model's constant for the version and its release history.
    /// * **A2L — always.** Its earliest published release, `01.01.00.00`, already has it.
    /// * **Everything else (X1E, unrecognized models) — assumed allowed.** No vendor source states
    ///   either way; the printer answers `result: "fail"` if it can't.
    ///
    /// Takes a [`QuirkContext`] so there is one answer to this question and not two that can
    /// disagree — the failure #240 fixed. Prefer
    /// [`PrinterClient::capabilities`](crate::client::PrinterClient::capabilities), which builds
    /// the context from cached telemetry for you.
    #[must_use]
    pub fn ams_remote_drying_support(&self, ctx: &QuirkContext) -> Support {
        self.dry.remote(ctx)
    }

    /// Returns whether an AMS drying cycle can run while a print is in progress, with its provenance.
    ///
    /// A separate, strictly narrower capability than
    /// [`ams_remote_drying_support`](Self::ams_remote_drying_support). During a print the firmware
    /// lowers the drying temperature below the printed filament's softening point; this crate does
    /// not reimplement that clamp. On an unsupported printer the firmware refuses mid-print with
    /// `dry_sf_reason` `0`.
    ///
    /// Sourced from the drying guide's "Introduction to Simultaneous Drying and Printing Function"
    /// list plus each model's release history ("Added support for printing while filament is
    /// drying" / "Print While Drying"). **Defaults to deny**, unlike idle remote drying. No `fun2`
    /// bit reports it, but a reported *clear* remote-dry bit refuses it.
    #[must_use]
    pub fn ams_drying_while_printing_support(&self, ctx: &QuirkContext) -> Support {
        self.dry.while_printing(ctx)
    }

    /// Returns the maximum safe nozzle/hotend temperature in °C for this model.
    #[must_use]
    pub fn nozzle_temp_max(&self) -> u16 {
        self.nozzle_temp_max
    }

    /// Returns the maximum safe heated bed temperature in °C for this model.
    ///
    /// `mains_220v` is `Some(true)`/`Some(false)` when the printer's mains voltage region is
    /// known (from `PrinterTelemetry::is_220v_power()`, derived from `home_flag` bit 3), or
    /// `None` before any `home_flag` telemetry has been received. Only the X1 and X1C have a
    /// voltage-dependent ceiling, per the official spec sheet ("Max Build Plate Temperature:
    /// 110°C @220V, 120°C @110V"); with the region unknown they return the lower one. Every
    /// other model ignores the parameter.
    #[must_use]
    pub fn bed_temp_max(&self, mains_220v: Option<bool>) -> u16 {
        match self.bed_temp_max {
            BedMax::Flat(max) => max,
            BedMax::Voltage { v220, v110 } => match mains_220v {
                Some(true) => v220,
                Some(false) => v110,
                None => v220.min(v110),
            },
        }
    }

    /// Returns this model's active chamber heater ceiling in °C (M141), or `None` if it has no active chamber heater [REF-MOTO-GCODE].
    ///
    /// Supported on: X1E, X2D, H2S, H2D, H2D Pro, H2C. One `Option` rather than a "has heater"
    /// flag plus a ceiling, so the two facts can't be stated inconsistently.
    #[must_use]
    pub fn chamber_heater_temp_max(&self) -> Option<u16> {
        self.chamber_heater_temp_max
    }
}

impl PrinterModel {
    /// Returns the [`ModelQuirks`] for this model variant.
    ///
    /// This is the single dispatch point — all model-specific behavior goes through the row
    /// returned here, rather than match-blocks scattered across the crate.
    pub fn quirks(&self) -> &'static ModelQuirks {
        self.spec_quirks().unwrap_or(&models::unknown::UNKNOWN)
    }
}

/// Logs a warning if `model` is [`PrinterModel::Unknown`], whose quirks are a conservative fallback.
///
/// Called where a client is constructed, not from [`PrinterModel::quirks`], which runs several
/// times per telemetry frame and would repeat the warning at that rate.
pub(crate) fn warn_if_unknown_model(model: PrinterModel) {
    if model == PrinterModel::Unknown {
        log::warn!(
            "Unrecognized printer model — falling back to conservative quirks; travel \
             and temperature limits (180mm axes, 80C bed, 300C nozzle) are the floor of \
             the supported family and will be below this machine's real ceilings"
        );
    }
}
// ============================================================================
// Specialized Telemetry Signal Processing Helpers
// ============================================================================

/// Generates a relative Z-axis movement G-code block, bounded by a client-side `z_max` distance
/// cap on the single move (not true position-aware crash prevention — the printer reports no
/// absolute axis position over MQTT, so neither firmware nor client can clamp from actual
/// position; this only bounds how far one relative command can travel). The move is wrapped in
/// BambuStudio's own jog sequence (`DevAxisCtrl.cpp`): `M211 S` saves the soft-endstop state,
/// `M211 X1 Y1 Z1` enables it, and `M211 R` restores the saved state afterwards, so the
/// printer's `M211` setting is left as it was. Per real H2D hardware testing (bambuddy #2579,
/// confirmed 2026-07-16) firmware does not enforce software travel limits on G-code received
/// over MQTT regardless of `M211` state — it provides no actual crash protection here.
///
/// Returns an empty string if `distance` is zero or exceeds the model's Z bounds.
pub(crate) fn format_z_move_gcode(distance: f32, feedrate: u32, z_max: f32) -> String {
    if !distance.is_finite() || distance == 0.0 || distance.abs() > z_max {
        return String::new();
    }
    format!(
        "M211 S\nM211 X1 Y1 Z1\nM1002 push_ref_mode\nG91\nG0 Z{:.2} F{}\nG90\nM1002 pop_ref_mode\nM211 R",
        distance, feedrate
    )
}

/// Generates a relative X/Y-axis movement G-code block, bounded by a client-side `axis_max`
/// distance cap on the single move — same limitation as `format_z_move_gcode`'s
/// `z_max` cap (not position-aware crash prevention; see its doc comment). No `M211`/reference-
/// mode wrapping here: that's specific to Z's frame-shifting risk (see `format_z_move_gcode`),
/// not applicable to X/Y.
///
/// Returns an empty string if `distance` is zero, non-finite, or exceeds `axis_max`.
pub(crate) fn format_xy_move_gcode(
    axis: char,
    distance: f32,
    feedrate: u32,
    axis_max: f32,
) -> String {
    if !distance.is_finite() || distance == 0.0 || distance.abs() > axis_max {
        return String::new();
    }
    format!("G91\nG0 {axis}{distance:.2} F{feedrate}\nG90")
}

pub(crate) const FAN_STEP_MAX: u8 = 15;
pub(crate) const FAN_ROUNDING_OFFSET: u32 = 7;
/// Largest percentage change one fan step can produce (`ceil(100 / FAN_STEP_MAX)`); a bigger
/// jump bypasses [`FanSpeedDebouncer`].
pub(crate) const DEBOUNCE_BYPASS_DIFF_PCT: i16 =
    (100 + FAN_STEP_MAX as i16 - 1) / FAN_STEP_MAX as i16;
/// Consecutive frames a one-step change must persist before [`FanSpeedDebouncer`] commits it.
pub(crate) const DEBOUNCE_CONFIRM_FRAMES: u8 = 3;

/// Converts a discrete fan speed step (0 to 15) to an integer percentage (0 to 100) [REF-CLIM-FANS].
///
/// Implements standard mathematical rounding logic: `Round(Step * 100 / 15)`.
pub fn fan_step_to_percentage(step: u8) -> u8 {
    if step >= FAN_STEP_MAX {
        100
    } else {
        ((step as u32 * 100 + FAN_ROUNDING_OFFSET) / FAN_STEP_MAX as u32) as u8
    }
}

/// Decodes a raw fan-speed telemetry string (`cooling_fan_speed`/`big_fan1_speed`/ `big_fan2_speed`/`heatbreak_fan_speed`) into a 0-100 percentage via [`fan_step_to_percentage()`].
/// Returns `None` if `raw` is absent or not a valid `u8`.
pub fn decode_fan_percentage(raw: Option<&str>) -> Option<u8> {
    let step: u8 = raw?.parse().ok()?;
    Some(fan_step_to_percentage(step))
}

/// Filters out transient quantization oscillation artifacts emitted by physical fan controllers.
///
/// **Why this is required [REF-CLIM-FANS]:**
/// Due to the low-resolution 0–15 PWM mapping on physical boards, minor fan throttle drift
/// can cause telemetry reports to bounce rapidly between adjacent steps (e.g. step 7 and step 8),
/// triggering interface flickering. This state tracker dampens steps by requiring persistent,
/// consecutive readings before committing a one-step change.
#[derive(Debug, Clone)]
pub struct FanSpeedDebouncer {
    last_stable_percentage: u8,
    consecutive_counts: u8,
    target_value: u8,
}

impl Default for FanSpeedDebouncer {
    fn default() -> Self {
        Self::new()
    }
}

impl FanSpeedDebouncer {
    /// Instantiates a new debouncer initialized to 0% speed.
    pub fn new() -> Self {
        Self {
            last_stable_percentage: 0,
            consecutive_counts: 0,
            target_value: 0,
        }
    }

    /// Processes an raw incoming fan speed percentage, filtering minor step oscillations.
    ///
    /// Allows large transitions (greater than 1 step or ~7% diff) to commit immediately
    /// to maintain user responsiveness, while locking single-step toggles until they persist
    /// for at least 3 consecutive frames.
    pub fn debounce(&mut self, incoming_percentage: u8) -> u8 {
        let diff = (incoming_percentage as i16 - self.last_stable_percentage as i16).abs();

        if diff <= DEBOUNCE_BYPASS_DIFF_PCT {
            // Evaluates whether the change is a transient step bounce or a permanent shift.
            if incoming_percentage == self.last_stable_percentage {
                self.consecutive_counts = 0;
                self.target_value = incoming_percentage;
            } else if incoming_percentage == self.target_value {
                self.consecutive_counts += 1;
                if self.consecutive_counts >= DEBOUNCE_CONFIRM_FRAMES {
                    self.last_stable_percentage = incoming_percentage;
                    self.consecutive_counts = 0;
                }
            } else {
                self.target_value = incoming_percentage;
                self.consecutive_counts = 1;
            }
        } else {
            // Significant control shift (e.g., fan turning off/on completely). Bypass filter.
            self.last_stable_percentage = incoming_percentage;
            self.target_value = incoming_percentage;
            self.consecutive_counts = 0;
        }

        self.last_stable_percentage
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::CameraProtocol;
    use crate::models::PrinterModel;

    /// A reported `fun2` bit 5 outranks every model rule, in both directions. Checked against a
    /// firmware-gated model and against the P1 override, so neither can quietly stop honoring
    /// the report (#240).
    #[test]
    fn test_reported_fun2_bit_outranks_model_rules() {
        let x1c = PrinterModel::X1C.quirks();
        let p1s = PrinterModel::P1S.quirks();

        // Bit 5 set beats the X1C's and the P1's never rules.
        let reported_yes = QuirkContext::empty().with_fun2(Some("20"));
        assert!(x1c.ams_remote_drying_support(&reported_yes).is_supported());
        assert!(p1s.ams_remote_drying_support(&reported_yes).is_supported());

        // Bit 5 clear beats a met firmware minimum.
        let reported_no = QuirkContext::empty()
            .with_fun2(Some("00"))
            .with_firmware(Some("99.99.99.99"));
        assert!(!x1c.ams_remote_drying_support(&reported_no).is_supported());

        // A fun2 carrying no hex digits is "didn't say", not a reported zero, so the model
        // rules still decide — which for both of these is never.
        let said_nothing = QuirkContext::empty()
            .with_fun2(Some(""))
            .with_firmware(Some("99.99.99.99"));
        assert!(!x1c.ams_remote_drying_support(&said_nothing).is_supported());
        assert!(!p1s.ams_remote_drying_support(&said_nothing).is_supported());
    }

    /// The model rules, which are what actually run on hardware — `fun2` is BambuStudio-only and
    /// absent on P1/A1 entirely, so these paths carry the weight. Sourced from the vendor release
    /// histories and drying guide (`reference/05_materials_ams.md` §5.4).
    #[test]
    fn test_model_rules_without_a_reported_bit() {
        let none = QuirkContext::empty();
        let newest = QuirkContext::empty().with_firmware(Some("99.99.99.99"));

        // Never, regardless of firmware: A1 (no remote-dry command path), P1 (screen-only), X1/X1C
        // (named unsupported by the drying guide).
        for model in [
            PrinterModel::A1,
            PrinterModel::A1Mini,
            PrinterModel::P1P,
            PrinterModel::P1S,
            PrinterModel::X1C,
            PrinterModel::X1,
        ] {
            assert_eq!(
                model.quirks().ams_remote_drying_support(&none),
                Support::Inferred(false),
                "{model:?}"
            );
            assert!(
                !model
                    .quirks()
                    .ams_remote_drying_support(&newest)
                    .is_supported(),
                "{model:?} must stay false even on the newest firmware"
            );
        }

        // Firmware-gated: at the threshold allows, just below refuses, unread is assumed yes.
        for (model, min, below) in [
            (PrinterModel::P2S, "01.02.00.00", "01.01.99.99"),
            (PrinterModel::H2D, "01.03.00.00", "01.02.99.99"),
            (PrinterModel::H2DPro, "01.02.00.00", "01.01.99.99"),
            (PrinterModel::H2S, "01.02.00.00", "01.01.99.99"),
            (PrinterModel::H2C, "01.02.00.00", "01.01.99.99"),
        ] {
            let at = QuirkContext::empty().with_firmware(Some(min));
            assert_eq!(
                model.quirks().ams_remote_drying_support(&at),
                Support::Inferred(true),
                "{model:?} at its minimum {min}"
            );
            let ctx = QuirkContext::empty().with_firmware(Some(below));
            assert_eq!(
                model.quirks().ams_remote_drying_support(&ctx),
                Support::Inferred(false),
                "{model:?} at {below} is below its minimum"
            );

            // An unread version is "nobody asked", not "too old": only a version actually read
            // and judged refuses, so capability never depends on call ordering. Deliberately
            // unlike bambuddy, whose `bool(firmware and ...)` is a Python None-guard on a
            // background scheduler — see `DryRule::Firmware`.
            assert_eq!(
                model.quirks().ams_remote_drying_support(&none),
                Support::Assumed(true),
                "{model:?} with no firmware read"
            );
        }

        // Earliest published release already has it: an unread version is inferred, not assumed.
        for model in [PrinterModel::X2D, PrinterModel::A2L] {
            assert_eq!(
                model.quirks().ams_remote_drying_support(&none),
                Support::Inferred(true),
                "{model:?}"
            );
        }

        // No vendor source either way: assumed allowed.
        assert_eq!(
            PrinterModel::X1E.quirks().ams_remote_drying_support(&none),
            Support::Assumed(true)
        );
    }

    /// Drying while printing defaults to deny and has no reporting bit of its own; a clear
    /// remote-dry bit still refuses it, a set one proves nothing.
    #[test]
    fn test_drying_while_printing_rules() {
        let none = QuirkContext::empty();

        for model in [
            PrinterModel::A1,
            PrinterModel::A1Mini,
            PrinterModel::P1P,
            PrinterModel::P1S,
            PrinterModel::X1C,
            PrinterModel::X1,
        ] {
            assert_eq!(
                model.quirks().ams_drying_while_printing_support(&none),
                Support::Inferred(false),
                "{model:?}"
            );
        }

        for (model, min, below) in [
            (PrinterModel::P2S, "01.02.00.00", "01.01.99.99"),
            (PrinterModel::H2D, "01.03.00.00", "01.02.99.99"),
            (PrinterModel::H2DPro, "01.02.00.00", "01.01.99.99"),
            (PrinterModel::H2S, "01.02.00.00", "01.01.99.99"),
            (PrinterModel::H2C, "01.02.00.00", "01.01.99.99"),
        ] {
            let q = model.quirks();
            assert_eq!(
                q.ams_drying_while_printing_support(&none),
                Support::Assumed(false),
                "{model:?} with no firmware read defaults to deny"
            );
            let at = QuirkContext::empty().with_firmware(Some(min));
            assert!(
                q.ams_drying_while_printing_support(&at).is_supported(),
                "{model:?} at {min}"
            );
            let old = QuirkContext::empty().with_firmware(Some(below));
            assert!(
                !q.ams_drying_while_printing_support(&old).is_supported(),
                "{model:?} at {below}"
            );
        }

        for model in [PrinterModel::X2D, PrinterModel::A2L] {
            assert_eq!(
                model.quirks().ams_drying_while_printing_support(&none),
                Support::Inferred(true),
                "{model:?}"
            );
        }
        for model in [PrinterModel::X1E, PrinterModel::Unknown] {
            assert_eq!(
                model.quirks().ams_drying_while_printing_support(&none),
                Support::Assumed(false),
                "{model:?}"
            );
        }

        let h2s = PrinterModel::H2S.quirks();
        let clear = QuirkContext::empty()
            .with_fun2(Some("00"))
            .with_firmware(Some("99.99.99.99"));
        assert_eq!(
            h2s.ams_drying_while_printing_support(&clear),
            Support::Reported(false)
        );
        let set = QuirkContext::empty().with_fun2(Some("20"));
        assert_eq!(
            h2s.ams_drying_while_printing_support(&set),
            Support::Assumed(false),
            "a set idle bit is not evidence for the concurrent case"
        );
    }

    #[test]
    fn test_fan_step_rounding() {
        assert_eq!(fan_step_to_percentage(0), 0);
        assert_eq!(fan_step_to_percentage(15), 100);
        assert_eq!(fan_step_to_percentage(8), 53);
        assert_eq!(fan_step_to_percentage(4), 27);
    }

    #[test]
    fn test_fan_debounce_filter() {
        let mut debouncer = FanSpeedDebouncer::new();

        assert_eq!(debouncer.debounce(53), 53);

        assert_eq!(debouncer.debounce(47), 53);
        assert_eq!(debouncer.debounce(47), 53);

        assert_eq!(debouncer.debounce(47), 47);

        assert_eq!(debouncer.debounce(53), 47);
        assert_eq!(debouncer.debounce(47), 47);
    }

    // Per-model quirks assertion tests

    #[test]
    fn test_a1_quirks() {
        let q = PrinterModel::A1.quirks();
        assert!(q.uses_plaintext_ftps_data_channel());
        assert!(!q.requires_ftps_tls_1_2());
        assert_eq!(q.door_sensor(), DoorSensor::None);
        assert_eq!(q.camera_protocol(), CameraProtocol::BinaryJpeg);
        assert!(!q.has_chamber_temperature_sensor());
        assert_eq!(q.chamber_heater_temp_max(), None);
        assert_eq!(q.physical_nozzle_count(), 1);
        assert!(!q.supports_nozzle_offset_calibration());
        assert!(!q.is_bed_on_z());
        assert!(!q.requires_wallclock_rtsp_timestamps());
        assert!(!q.has_auxiliary_left2_fan());
        assert!(!q.has_auxiliary_left_fan());
        assert!(!q.has_chamber_exhaust_fan());
        assert_eq!(q.build_volume().z, 256.0);
        assert_eq!(q.nozzle_temp_max(), 300);
        assert_eq!(q.bed_temp_max(None), 100);
        assert!(!q.supports_airduct_mode());
        assert!(q.supports_prompt_sound());
        assert!(!q.has_buzzer());
    }

    #[test]
    fn test_a2l_quirks() {
        let q = PrinterModel::A2L.quirks();
        assert!(!q.uses_plaintext_ftps_data_channel());
        assert!(!q.requires_ftps_tls_1_2());
        assert_eq!(q.door_sensor(), DoorSensor::None);
        assert_eq!(q.camera_protocol(), CameraProtocol::BinaryJpeg);
        assert!(!q.has_chamber_temperature_sensor());
        assert_eq!(q.chamber_heater_temp_max(), None);
        assert_eq!(q.physical_nozzle_count(), 1);
        assert!(!q.supports_nozzle_offset_calibration());
        assert!(!q.is_bed_on_z());
        assert_eq!(q.build_volume().z, 325.0);
        assert!(q.relative_z_move_gcode(330.0, 3000).is_empty());
        assert!(!q.relative_z_move_gcode(300.0, 3000).is_empty());
        assert_eq!(q.nozzle_temp_max(), 300);
        assert_eq!(q.bed_temp_max(None), 80);
        assert!(!q.has_auxiliary_left_fan());
        assert!(!q.has_chamber_exhaust_fan());
        assert!(!q.supports_airduct_mode());
        assert!(q.supports_prompt_sound());
        assert!(!q.has_buzzer());
    }

    #[test]
    fn test_a1_mini_quirks() {
        let q = PrinterModel::A1Mini.quirks();
        assert!(q.uses_plaintext_ftps_data_channel());
        assert!(!q.requires_ftps_tls_1_2());
        assert_eq!(q.door_sensor(), DoorSensor::None);
        assert_eq!(q.camera_protocol(), CameraProtocol::BinaryJpeg);
        assert!(!q.has_chamber_temperature_sensor());
        assert_eq!(q.chamber_heater_temp_max(), None);
        assert_eq!(q.physical_nozzle_count(), 1);
        assert!(!q.supports_nozzle_offset_calibration());
        assert!(!q.is_bed_on_z());
        assert_eq!(q.build_volume().z, 180.0);
        assert!(q.relative_z_move_gcode(200.0, 3000).is_empty());
        assert!(!q.relative_z_move_gcode(150.0, 3000).is_empty());
        assert_eq!(q.nozzle_temp_max(), 300);
        assert_eq!(q.bed_temp_max(None), 80);
        assert!(!q.has_auxiliary_left_fan());
        assert!(!q.has_chamber_exhaust_fan());
        assert!(!q.supports_airduct_mode());
        assert!(q.supports_prompt_sound());
        assert!(!q.has_buzzer());
    }

    #[test]
    fn test_p1_quirks() {
        for model in [PrinterModel::P1P, PrinterModel::P1S] {
            let q = model.quirks();
            assert!(!q.uses_plaintext_ftps_data_channel());
            assert!(!q.requires_ftps_tls_1_2());
            assert_eq!(q.door_sensor(), DoorSensor::None);
            assert_eq!(q.camera_protocol(), CameraProtocol::BinaryJpeg);
            assert!(!q.has_chamber_temperature_sensor());
            assert_eq!(q.chamber_heater_temp_max(), None);
            assert_eq!(q.physical_nozzle_count(), 1);
            assert!(!q.supports_nozzle_offset_calibration());
            assert!(q.is_bed_on_z());
            assert!(!q.requires_wallclock_rtsp_timestamps());
            assert!(!q.has_auxiliary_left2_fan());
            assert_eq!(q.has_auxiliary_left_fan(), model == PrinterModel::P1S);
            assert!(!q.has_chamber_exhaust_fan());
            assert_eq!(q.build_volume().z, 256.0);
            assert_eq!(q.nozzle_temp_max(), 300);
            assert_eq!(q.bed_temp_max(None), 100);
            assert!(!q.supports_airduct_mode());
            assert!(!q.supports_prompt_sound());
            assert!(!q.has_buzzer());
        }
    }

    #[test]
    fn test_p2s_quirks() {
        let q = PrinterModel::P2S.quirks();
        assert!(!q.uses_plaintext_ftps_data_channel());
        assert!(q.requires_ftps_tls_1_2());
        assert_eq!(q.door_sensor(), DoorSensor::Stat);
        assert_eq!(q.camera_protocol(), CameraProtocol::Rtsps);
        assert!(q.has_chamber_temperature_sensor());
        assert_eq!(q.chamber_heater_temp_max(), None);
        assert_eq!(q.physical_nozzle_count(), 1);
        assert!(!q.supports_nozzle_offset_calibration());
        assert!(q.is_bed_on_z());
        assert!(q.requires_wallclock_rtsp_timestamps());
        assert!(q.has_auxiliary_left2_fan());
        assert!(q.has_auxiliary_left_fan());
        assert!(!q.has_chamber_exhaust_fan());
        assert_eq!(q.build_volume().z, 256.0);
        assert_eq!(q.nozzle_temp_max(), 300);
        assert_eq!(q.bed_temp_max(None), 110);
        assert!(q.supports_airduct_mode());
        assert!(!q.supports_prompt_sound());
        assert!(!q.has_buzzer());
    }

    #[test]
    fn test_x1c_quirks() {
        let q = PrinterModel::X1C.quirks();
        assert!(!q.uses_plaintext_ftps_data_channel());
        assert!(!q.requires_ftps_tls_1_2());
        assert_eq!(q.door_sensor(), DoorSensor::HomeFlag);
        assert_eq!(q.camera_protocol(), CameraProtocol::Rtsps);
        assert!(q.has_chamber_temperature_sensor());
        assert_eq!(q.chamber_heater_temp_max(), None);
        assert_eq!(q.physical_nozzle_count(), 1);
        assert!(!q.supports_nozzle_offset_calibration());
        assert!(q.is_bed_on_z());
        assert!(!q.requires_wallclock_rtsp_timestamps());
        assert!(!q.has_auxiliary_left2_fan());
        assert!(q.has_auxiliary_left_fan());
        assert!(!q.has_chamber_exhaust_fan());
        assert_eq!(q.build_volume().z, 256.0);
        assert_eq!(q.nozzle_temp_max(), 300);
        assert_eq!(q.bed_temp_max(Some(true)), 110);
        assert_eq!(q.bed_temp_max(Some(false)), 120);
        assert_eq!(q.bed_temp_max(None), 110);
        assert!(!q.supports_airduct_mode());
        assert!(!q.supports_prompt_sound());
        assert!(!q.has_buzzer());
    }

    #[test]
    fn test_x1_quirks_match_x1c_except_aux_fan() {
        let x1 = PrinterModel::X1.quirks();
        let x1c = PrinterModel::X1C.quirks();
        assert!(!x1.has_auxiliary_left_fan());
        assert_eq!(x1.nozzle_temp_max(), x1c.nozzle_temp_max());
        for mains in [Some(true), Some(false), None] {
            assert_eq!(x1.bed_temp_max(mains), x1c.bed_temp_max(mains));
        }
        assert_eq!(x1.build_volume().z, x1c.build_volume().z);
        assert_eq!(x1.camera_protocol(), x1c.camera_protocol());
        assert_eq!(x1.chamber_heater_temp_max(), None);
        assert_eq!(x1.physical_nozzle_count(), 1);
        assert!(x1.is_bed_on_z());
        assert_eq!(x1.door_sensor(), DoorSensor::HomeFlag);
    }

    #[test]
    fn test_x1e_quirks() {
        let q = PrinterModel::X1E.quirks();
        assert!(!q.uses_plaintext_ftps_data_channel());
        assert!(!q.requires_ftps_tls_1_2());
        assert_eq!(q.door_sensor(), DoorSensor::HomeFlag);
        assert_eq!(q.camera_protocol(), CameraProtocol::Rtsps);
        assert!(q.has_chamber_temperature_sensor());
        assert_eq!(q.chamber_heater_temp_max(), Some(60));
        assert_eq!(q.physical_nozzle_count(), 1);
        assert!(!q.supports_nozzle_offset_calibration());
        assert!(q.is_bed_on_z());
        assert_eq!(q.build_volume().z, 256.0);
        assert_eq!(q.nozzle_temp_max(), 320);
        assert_eq!(q.bed_temp_max(None), 110);
        assert!(q.has_auxiliary_left_fan());
        assert!(!q.has_chamber_exhaust_fan());
        assert!(!q.supports_airduct_mode());
        assert!(!q.supports_prompt_sound());
        assert!(!q.has_buzzer());
    }

    #[test]
    fn test_x2d_quirks() {
        let q = PrinterModel::X2D.quirks();
        assert!(!q.uses_plaintext_ftps_data_channel());
        assert!(q.requires_ftps_tls_1_2());
        assert_eq!(q.door_sensor(), DoorSensor::Stat);
        assert_eq!(q.camera_protocol(), CameraProtocol::Rtsps);
        assert!(q.has_chamber_temperature_sensor());
        assert_eq!(q.chamber_heater_temp_max(), Some(65));
        assert_eq!(q.physical_nozzle_count(), 2);
        assert!(q.supports_nozzle_offset_calibration());
        assert!(q.is_bed_on_z());
        assert!(q.has_auxiliary_left2_fan());
        assert!(q.has_auxiliary_left_fan());
        assert!(q.has_chamber_exhaust_fan());
        assert_eq!(q.build_volume().z, 256.0);
        assert_eq!(q.nozzle_temp_max(), 300);
        assert_eq!(q.bed_temp_max(None), 120);
        assert!(q.supports_airduct_mode());
        assert!(!q.supports_prompt_sound());
        assert!(!q.has_buzzer());
    }

    #[test]
    fn test_h2s_quirks() {
        let q = PrinterModel::H2S.quirks();
        assert!(!q.uses_plaintext_ftps_data_channel());
        assert!(!q.requires_ftps_tls_1_2());
        assert_eq!(q.door_sensor(), DoorSensor::Stat);
        assert_eq!(q.camera_protocol(), CameraProtocol::Rtsps);
        assert!(q.has_chamber_temperature_sensor());
        assert_eq!(q.chamber_heater_temp_max(), Some(65));
        assert_eq!(q.physical_nozzle_count(), 1);
        assert!(!q.supports_nozzle_offset_calibration());
        assert!(q.is_bed_on_z());
        assert_eq!(q.build_volume().z, 340.0);
        assert_eq!(q.nozzle_temp_max(), 350);
        assert_eq!(q.bed_temp_max(None), 120);
        assert!(q.has_auxiliary_left_fan());
        assert!(q.has_chamber_exhaust_fan());
        assert!(q.supports_airduct_mode());
        assert!(!q.supports_prompt_sound());
        assert!(q.has_buzzer());
    }

    #[test]
    fn test_h2d_quirks() {
        let q = PrinterModel::H2D.quirks();
        assert_eq!(q.chamber_heater_temp_max(), Some(65));
        assert_eq!(q.physical_nozzle_count(), 2);
        assert!(q.supports_nozzle_offset_calibration());
        assert!(q.is_bed_on_z());
        assert_eq!(q.camera_protocol(), CameraProtocol::Rtsps);
        assert_eq!(q.build_volume().z, 325.0);
        assert_eq!(q.nozzle_temp_max(), 350);
        assert_eq!(q.bed_temp_max(None), 120);
        assert!(q.has_auxiliary_left_fan());
        assert!(q.has_chamber_exhaust_fan());
        assert!(q.supports_airduct_mode());
        assert!(!q.supports_prompt_sound());
        assert!(q.has_buzzer());
    }

    #[test]
    fn test_h2d_pro_quirks() {
        let q = PrinterModel::H2DPro.quirks();
        assert_eq!(q.chamber_heater_temp_max(), Some(65));
        assert_eq!(q.physical_nozzle_count(), 2);
        assert!(q.supports_nozzle_offset_calibration());
        assert_eq!(q.camera_protocol(), CameraProtocol::Rtsps);
        assert_eq!(q.build_volume().z, 325.0);
        assert_eq!(q.nozzle_temp_max(), 350);
        assert_eq!(q.bed_temp_max(None), 120);
        assert!(q.has_auxiliary_left_fan());
        assert!(q.has_chamber_exhaust_fan());
        assert!(q.supports_airduct_mode());
        assert!(!q.supports_prompt_sound());
        assert!(q.has_buzzer());
    }

    #[test]
    fn test_h2c_quirks() {
        let q = PrinterModel::H2C.quirks();
        assert_eq!(q.chamber_heater_temp_max(), Some(65));
        assert_eq!(q.physical_nozzle_count(), 7);
        assert!(q.supports_nozzle_offset_calibration());
        assert!(q.is_bed_on_z());
        assert_eq!(q.camera_protocol(), CameraProtocol::Rtsps);
        assert_eq!(q.build_volume().z, 325.0);
        assert_eq!(q.nozzle_temp_max(), 350);
        assert_eq!(q.bed_temp_max(None), 120);
        assert!(q.has_auxiliary_left_fan());
        assert!(q.has_chamber_exhaust_fan());
        assert!(q.supports_airduct_mode());
        assert!(!q.supports_prompt_sound());
        assert!(q.has_buzzer());
    }

    #[test]
    fn test_unknown_fallback_quirks() {
        let q = PrinterModel::Unknown.quirks();
        assert_eq!(q.chamber_heater_temp_max(), None);
        assert_eq!(q.physical_nozzle_count(), 1);
        assert_eq!(q.camera_protocol(), CameraProtocol::Rtsps);
        assert!(q.has_auxiliary_left_fan());
        assert!(!q.has_chamber_exhaust_fan());

        // The fallback's ceilings must be the floor of the whole family, and must not vary
        // with mains region: a 110V unit used to inherit X1C's 120C bed ceiling, 40C above
        // the entry-level models an unrecognized printer might actually be.
        for mains in [Some(true), Some(false), None] {
            assert_eq!(q.bed_temp_max(mains), 80);
        }
        assert_eq!(q.nozzle_temp_max(), 300);
        assert_eq!(q.build_volume().z, 180.0);
        assert_eq!(q.build_volume().x, 180.0);
        assert_eq!(q.build_volume().y, 180.0);
        assert!(q.validate_gcode("G28 Z", None).is_err());
    }

    // Z-move gcode parameterization tests

    #[test]
    fn test_z_move_gcode_parameterized() {
        let gcode = format_z_move_gcode(10.0, 3000, 256.0);
        assert!(gcode.contains("Z10.00"));
        assert!(gcode.contains("F3000"));
        assert!(gcode.starts_with("M211 S\nM211 X1 Y1 Z1\n"));
        assert!(gcode.ends_with("M1002 pop_ref_mode\nM211 R"));
        assert!(gcode.contains("push_ref_mode"));
    }

    #[test]
    fn test_z_move_gcode_negative_distance() {
        let gcode = format_z_move_gcode(-5.5, 1500, 256.0);
        assert!(gcode.contains("Z-5.50"));
        assert!(gcode.contains("F1500"));
    }

    #[test]
    fn test_z_move_gcode_exceeds_bounds() {
        assert!(format_z_move_gcode(300.0, 3000, 256.0).is_empty());
        assert!(format_z_move_gcode(-300.0, 3000, 256.0).is_empty());
    }

    #[test]
    fn test_z_move_gcode_rejects_non_finite() {
        // Regression: NaN failed both the `== 0.0` and `.abs() > z_max` guards,
        // so a malformed G0 ZNaN command would have reached the printer.
        assert!(format_z_move_gcode(f32::NAN, 3000, 256.0).is_empty());
        assert!(format_z_move_gcode(f32::INFINITY, 3000, 256.0).is_empty());
        assert!(format_z_move_gcode(f32::NEG_INFINITY, 3000, 256.0).is_empty());
    }

    #[test]
    fn test_z_move_gcode_zero_distance() {
        assert!(format_z_move_gcode(0.0, 3000, 256.0).is_empty());
    }

    #[test]
    fn test_z_move_gcode_at_boundary() {
        let gcode = format_z_move_gcode(256.0, 3000, 256.0);
        assert!(gcode.contains("Z256.00"));
    }

    #[test]
    fn test_z_move_via_trait() {
        let q = PrinterModel::P1P.quirks();
        let gcode = q.relative_z_move_gcode(15.0, 2000);
        assert!(gcode.contains("Z15.00"));
        assert!(gcode.contains("F2000"));
    }

    // X/Y-move gcode parameterization tests

    #[test]
    fn test_xy_move_gcode_parameterized() {
        let gcode = format_xy_move_gcode('X', 10.0, 3000, 256.0);
        assert!(gcode.contains("G0 X10.00"));
        assert!(gcode.contains("F3000"));
        assert!(
            !gcode.contains("M211"),
            "X/Y moves don't need Z's M211/reference-mode wrapping"
        );
    }

    #[test]
    fn test_xy_move_gcode_exceeds_bounds() {
        assert!(format_xy_move_gcode('X', 300.0, 3000, 256.0).is_empty());
        assert!(format_xy_move_gcode('Y', -300.0, 3000, 256.0).is_empty());
    }

    #[test]
    fn test_xy_move_gcode_rejects_non_finite() {
        assert!(format_xy_move_gcode('X', f32::NAN, 3000, 256.0).is_empty());
        assert!(format_xy_move_gcode('Y', f32::INFINITY, 3000, 256.0).is_empty());
    }

    #[test]
    fn test_xy_move_gcode_zero_distance() {
        assert!(format_xy_move_gcode('X', 0.0, 3000, 256.0).is_empty());
    }

    #[test]
    fn test_xy_move_via_trait_rejects_non_xy_axis() {
        // format_xy_move_gcode itself is axis-agnostic (just formats whatever char it's given,
        // same as format_z_move_gcode is Z-only by construction) — the 'X'/'Y'-only restriction
        // lives in relative_xy_move_gcode's axis match, so test it there.
        let q = PrinterModel::P1P.quirks();
        assert!(q.relative_xy_move_gcode('Z', 10.0, 3000).is_empty());
    }

    #[test]
    fn test_xy_move_via_trait_uses_model_specific_bounds() {
        // A1 Mini's x_max/y_max is 180mm, unlike the 256mm default — confirms the trait method
        // routes through the model's own x_max()/y_max(), not a shared constant.
        let q = PrinterModel::A1Mini.quirks();
        assert!(q.relative_xy_move_gcode('X', 200.0, 2000).is_empty());
        let gcode = q.relative_xy_move_gcode('X', 100.0, 2000);
        assert!(gcode.contains("X100.00"));
    }

    // Homing safety tests

    #[test]
    fn test_unsafe_homing_bed_on_z() {
        let q = PrinterModel::P1P.quirks();
        assert!(q.validate_gcode("G28 Z", None).is_err());
        assert!(q.validate_gcode("g28 x", None).is_err());
        assert!(q.validate_gcode("G28 X Y Z", None).is_err());
        assert!(q.validate_gcode("G28", None).is_ok());
        assert!(q.validate_gcode("G1 Z10", None).is_ok());
        assert!(q.validate_gcode("G280 Z", None).is_ok());
        assert!(q.validate_gcode("", None).is_ok());
        assert!(q.validate_gcode("G28", None).is_ok());
        assert!(q.validate_gcode("G28 z", None).is_err());
    }

    #[test]
    fn test_unsafe_homing_hidden_on_later_line() {
        // Regression: the homing check used to inspect only the first
        // whitespace-split token of the whole string, so an unsafe G28 buried on a
        // later line of a multi-statement payload passed through unchecked.
        let q = PrinterModel::P1P.quirks();
        assert!(q.validate_gcode("M104 S200\nG28 Z", None).is_err());
    }

    #[test]
    fn test_unsafe_homing_glued_axis() {
        // Regression: "G28X" (no whitespace between the command and the axis
        // letter) used to fail the exact "G28" token match and pass through unchecked.
        let q = PrinterModel::P1P.quirks();
        assert!(q.validate_gcode("G28X", None).is_err());
    }

    #[test]
    fn test_unsafe_homing_ignores_trailing_comment() {
        // Regression (issue #55): a bare (all-axis) G28 with a trailing comment mentioning
        // X/Y/Z must not be mistaken for an explicit axis-constrained G28.
        let q = PrinterModel::P1P.quirks();
        assert!(
            q.validate_gcode("G28 ; home XYZ before print", None)
                .is_ok()
        );
        assert!(
            q.validate_gcode("G28 (home XYZ before print)", None)
                .is_ok()
        );
        // A genuine axis-constrained G28 before the comment must still be flagged.
        assert!(q.validate_gcode("G28 Z ; home Z only", None).is_err());
    }

    #[test]
    fn test_unsafe_homing_ignores_non_executable_g28() {
        // Regression (issue #100): the mirror image of #55 — the G28 *match itself* sitting in
        // text that is not executable G-code, either a comment that opened earlier on the line
        // or an M117 display message. The executable G-code on each of these lines is safe (or
        // absent), so none may be rejected.
        let q = PrinterModel::P1P.quirks();
        assert!(q.validate_gcode("; G28 Z", None).is_ok());
        assert!(q.validate_gcode("(G28 Z)", None).is_ok());
        assert!(q.validate_gcode("M400 ; then G28 Z manually", None).is_ok());
        assert!(q.validate_gcode("M117 G28 Z", None).is_ok());
        assert!(q.validate_gcode("  m117 now homing G28 Z", None).is_ok());
        // M1170 is a different command, not M117 with a trailing digit — do not swallow it.
        assert!(q.validate_gcode("M1170 G28 Z", None).is_err());
        // Only the commented line is inert; a real G28 Z on another line still counts.
        assert!(q.validate_gcode("; G28 Z\nG28 Z", None).is_err());
    }

    #[test]
    fn test_a1_homing_always_safe() {
        let q = PrinterModel::A1.quirks();
        assert!(q.validate_gcode("G28 Z", None).is_ok());
        assert!(q.validate_gcode("G28", None).is_ok());
    }

    #[test]
    fn test_calibration_mask_matches_p1s_capture() {
        // P1S capture: option 62 (bits 1-5) queued stages for bits 1-3 only. Bits 4 and 5
        // produced nothing while the firmware still acked "success" [REF-MQTT-LIFECYCLE].
        let q = PrinterModel::P1S.quirks();
        assert_eq!(q.supported_calibration_mask(), 0b0000_1110);
        assert_eq!(62 & q.supported_calibration_mask(), 14);
    }

    #[test]
    fn test_calibration_mask_never_includes_internal_bits() {
        // Bits 0 (xcam) and 6 (nozzle clumping) are internal; no model may advertise them.
        for model in [
            PrinterModel::P1S,
            PrinterModel::A1,
            PrinterModel::X1C,
            PrinterModel::Unknown,
        ] {
            let mask = model.quirks().supported_calibration_mask();
            assert_eq!(
                mask & 0b0100_0001,
                0,
                "{model:?} advertises an internal bit"
            );
        }
    }

    #[test]
    fn test_calibration_mask_tracks_nozzle_offset_quirk() {
        // Bit 4 must follow the existing predicate rather than being hardcoded.
        for model in [PrinterModel::P1S, PrinterModel::A1, PrinterModel::X1C] {
            let q = model.quirks();
            assert_eq!(
                q.supported_calibration_mask() & 0b0001_0000 != 0,
                q.supports_nozzle_offset_calibration(),
                "{model:?} bit 4 disagrees with supports_nozzle_offset_calibration()"
            );
        }
    }

    #[test]
    fn test_heatbed_thermal_calibration_defaults_unsupported() {
        // Fail-safe direction: the wire acks success either way, so absent a capture showing a
        // queued stage a model is assumed not to run it.
        assert!(
            !PrinterModel::Unknown
                .quirks()
                .supports_heatbed_thermal_calibration()
        );
        assert!(
            !PrinterModel::P1S
                .quirks()
                .supports_heatbed_thermal_calibration()
        );
    }
}
