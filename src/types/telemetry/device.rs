//! Device-level hardware telemetry (extruders, nozzles, bed, fans, airduct, CTC, cameras).

#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::merge::{Mergeable, keep_new, merge_opt};
use serde::{Deserialize, Serialize};

use super::diagnostics::CtcTelemetry;
use super::temps::{HeaterTemps, unpack_temperature};

/// Device hardware state properties containing physical tooling descriptions.
///
/// Appears at two locations on the wire:
/// - Top-level `{"device": {...}}` for incremental updates (e.g., `push_alt_nozzle_info`)
/// - Nested inside `{"print": {"device": {...}}}` for pushall on H2/P2/X2 models
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceTelemetry {
    /// Structured descriptions representing the active extruder assembly properties.
    pub nozzle: Option<NozzleCollection>,

    /// Per-extruder thermal and routing state for IDEX platforms [REF-THER-DECODE §Dual-Extruder].
    pub extruder: Option<ExtruderCollection>,

    /// Nested structures tracking cooling components and climate routing [REF-CLIM-FANS].
    pub airduct: Option<AirductCollection>,

    /// Chamber Temperature Controller telemetry [REF-THER-DECODE].
    pub ctc: Option<CtcTelemetry>,

    /// Composite-packed bed temperature on H2/P2/X2 models.
    #[serde(default)]
    pub bed: Option<BedTelemetry>,

    /// Laser/cutter tool mount state.
    #[serde(default)]
    pub ext_tool: Option<ExtToolTelemetry>,

    /// Fire alarm/extinguisher status (H2D Pro, H2S).
    #[serde(default)]
    pub fire_ext: Option<serde_json::Value>,

    /// Composite-packed bed temperature mirroring `bed.info.temp`; confirmed redundant, not a fallback.
    ///
    /// A fixture payload carries the identical value in both fields, and both
    /// pybambu (`models.py`, reads only `device.bed.info.temp`) and bambuddy independently
    /// never consult this field either. Parsed for wire-format completeness only —
    /// The bed-temperature decode deliberately does not read it.
    #[serde(default)]
    pub bed_temp: Option<u32>,
}

impl Mergeable for DeviceTelemetry {
    /// Merges a freshly-parsed `DeviceTelemetry` into `self` field-by-field, instead of
    /// replacing `self` wholesale.
    ///
    /// Same shape as `AmsStatusReport::merge_from` one struct up — a
    /// `device` push touching only one sub-object (e.g. `ctc`) has every other field simply
    /// absent from that message, not explicitly cleared. Replacing `self` wholesale on any
    /// `Some(_)` push wiped the other cached sub-objects (`nozzle`, `extruder`, `airduct`,
    /// `bed`, `ext_tool`) back to `None`.
    ///
    /// Recurses into `nozzle`/`extruder`/`airduct` rather than replacing them
    /// wholesale when both sides have `Some(_)` — confirmed via `pybambu` and `bambuddy`
    /// (see each collection's own `merge_from`) that `device.nozzle.info`,
    /// `device.extruder.info`, and `device.airduct.modeCur`/`modeList`/`parts` can each be
    /// absent independent of their parent sub-object arriving.
    ///
    /// Recurses into `ctc` too — confirmed via BambuStudio's own
    /// `DevChamber::ParseChamberV2_0` (see `CtcTelemetry::merge_from`).
    ///
    /// Recurses into `ext_tool` too — confirmed via BambuStudio's own
    /// `DevExtensionToolParser::ParseV2_0` (see `ExtToolTelemetry::merge_from`).
    ///
    /// Recurses into `bed` too — confirmed via BambuStudio's
    /// `json_diff::restore_objects` generic reconstruction layer (see `BedTelemetry::merge_from`
    /// for the full trace).
    fn merge_from(&mut self, incoming: &Self) {
        let Self {
            nozzle,
            extruder,
            airduct,
            ctc,
            bed,
            ext_tool,
            fire_ext,
            bed_temp,
        } = incoming;
        merge_opt(&mut self.nozzle, nozzle);
        merge_opt(&mut self.extruder, extruder);
        merge_opt(&mut self.airduct, airduct);
        merge_opt(&mut self.ctc, ctc);
        merge_opt(&mut self.bed, bed);
        merge_opt(&mut self.ext_tool, ext_tool);
        keep_new(&mut self.fire_ext, fire_ext);
        keep_new(&mut self.bed_temp, bed_temp);
    }
}

/// Bed telemetry sub-object from `device.bed` on new-protocol printers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BedTelemetry {
    /// Bed info containing composite-packed temperature.
    #[serde(default)]
    pub info: Option<BedInfo>,
    /// Bed heating state (2 = heating).
    #[serde(default)]
    pub state: Option<u32>,
}

impl Mergeable for BedTelemetry {
    /// Merges a freshly-parsed `BedTelemetry` into `self` field-by-field.
    ///
    /// Confirmed against BambuStudio's `json_diff::restore_objects`
    /// (`src/slic3r/Utils/json_diff.cpp`), wired into `MachineObject::parse_json` for any
    /// message tagged `print.msg == 1` ("diff message" — confirmed live in real P1S traffic
    /// via `tests/mocks/P1S_print_sequence.ndjson`'s `print.msg` values `0`/`1`). Before any
    /// field-specific parser runs, `restore_objects` recursively reconstructs the entire
    /// payload against the last-known full state: for every nested object at every depth,
    /// each leaf field independently takes the incoming value if present, otherwise the
    /// cached one. Traced end-to-end in both BambuStudio and OrcaSlicer (identical, not
    /// diverged): `parse_json` → `diff2all` → `jj = j["print"]` → `parse_new_info(jj)` →
    /// `device = print["device"]` → `DevBed::ParseV2_0(device, m_bed)` all operate on the
    /// already-reconstructed tree — so `device.bed.info`/`device.bed.state` each survive a
    /// partial push independently of each other, the same as `ctc`'s `info`/`state`,
    /// even though no field-specific parser (`DevBed.cpp`) ever reads the nested object at
    /// all — the reconstruction layer preserves it regardless of whether anything downstream
    /// consumes it.
    fn merge_from(&mut self, incoming: &Self) {
        let Self { info, state } = incoming;
        keep_new(&mut self.info, info);
        keep_new(&mut self.state, state);
    }
}

/// Bed info segment with composite-packed temperature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BedInfo {
    /// Composite-packed bed temperature [REF-THER-DECODE].
    #[serde(default)]
    pub temp: Option<u32>,
}

impl BedInfo {
    /// The bed's temperatures unpacked from `temp`; `None` when it is absent.
    #[must_use]
    pub fn temperatures(&self) -> Option<HeaterTemps> {
        Some(unpack_temperature(f64::from(self.temp?)))
    }
}

/// Laser/cutter external tool telemetry from `device.ext_tool`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtToolTelemetry {
    /// Mount state (0 = not mounted, 1 = mounted).
    #[serde(default)]
    pub mount: Option<i32>,
    /// Tool type code (e.g. `"LB00"` = 10W laser, `"LB01"` = 40W laser, `"CP00"` = cutter).
    #[serde(default, rename = "type")]
    pub tool_type: Option<String>,
    /// Calibration state.
    #[serde(default)]
    pub calib: Option<i32>,
    /// Low-precision mode flag.
    #[serde(default)]
    pub low_prec: Option<bool>,
    /// Thermal head temperature.
    #[serde(default)]
    pub th_temp: Option<i32>,
    /// 3D mount state.
    #[serde(default)]
    pub mount_3d: Option<i32>,
}

impl Mergeable for ExtToolTelemetry {
    /// Merges a freshly-parsed `ExtToolTelemetry` into `self` field-by-field.
    ///
    /// Confirmed against BambuStudio's own `DevExtensionToolParser::ParseV2_0`
    /// (`src/slic3r/GUI/DeviceCore/DevExtensionTool.cpp`) for `mount_3d`/`calib` (both parsed
    /// via `DevJsonValParser::ParseVal`'s current-value-as-default overload — absent leaves
    /// the previous value untouched) and `type`/`tool_type` (absent/unrecognized falls
    /// through the type-map lookup without writing `m_tool_type` at all). `mount`/`low_prec`/
    /// `th_temp` aren't modeled by BambuStudio at all — extended uniformly here for
    /// consistency with every other `merge_from` in this file, same as `DeviceTelemetry::merge_from` extending
    /// nozzle/extruder/airduct together once the pattern was established for one.
    fn merge_from(&mut self, incoming: &Self) {
        let Self {
            mount,
            tool_type,
            calib,
            low_prec,
            th_temp,
            mount_3d,
        } = incoming;
        keep_new(&mut self.mount, mount);
        keep_new(&mut self.tool_type, tool_type);
        keep_new(&mut self.calib, calib);
        keep_new(&mut self.low_prec, low_prec);
        keep_new(&mut self.th_temp, th_temp);
        keep_new(&mut self.mount_3d, mount_3d);
    }
}

/// Wrap block holding nozzle characteristics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NozzleCollection {
    /// Polymorphic array representing active carriages and tool configurations.
    ///
    /// `None` means this push's `info` key was absent from the wire — leave previously cached
    /// entries untouched. `Some(vec![])` means the key was present but empty, which (per
    /// `NozzleCollection::merge_from`) replaces the cached entries with an empty list.
    /// Confirmed against BambuStudio's `json_diff::restore_objects` (`src/slic3r/Utils/
    /// json_diff.cpp`) — its generic recursive JSON-delta merge treats a present array
    /// differing from the last-known value as the new authoritative value (including an empty
    /// array replacing a non-empty one), and only an absent key as "carry the old value
    /// forward." `#[serde(default)]` on `Option<Vec<_>>` gives this distinction for free
    /// (absent key -> `None`, present key -> `Some(_)` however short) — previously both
    /// collapsed to the same empty `Vec` (same shape as the `AmsTray` fix).
    #[serde(default)]
    pub info: Option<Vec<NozzleInfo>>,

    /// Bitmask of physically present nozzle IDs (HotendRack).
    #[serde(default)]
    pub exist: Option<u32>,

    /// Nozzle state bitmask.
    #[serde(default)]
    pub state: Option<u32>,

    /// Tool-change source nozzle ID.
    #[serde(default)]
    pub src_id: Option<u32>,

    /// Tool-change target nozzle ID.
    #[serde(default)]
    pub tar_id: Option<u32>,
}

impl Mergeable for NozzleCollection {
    /// Merges a freshly-parsed `NozzleCollection` into `self` field-by-field.
    ///
    /// Confirmed via `pybambu` and `bambuddy` (see `DeviceTelemetry::merge_from`) —
    /// `device.nozzle.info` can be absent from a push while sibling `device.nozzle` fields
    /// change, and must not be treated as "nozzle info cleared." `info` is now
    /// `Option<Vec<_>>` (see its doc comment) so a present-but-empty push actually clears —
    /// wholesale replace, not a keyed per-entry merge, matching BambuStudio's own
    /// `DevNozzleSystemParser::ParseV2_0` (`system->ClearNozzles()` + full rebuild whenever
    /// `nozzle.info` is present in the already-reconstructed snapshot).
    fn merge_from(&mut self, incoming: &Self) {
        let Self {
            info,
            exist,
            state,
            src_id,
            tar_id,
        } = incoming;
        keep_new(&mut self.info, info);
        keep_new(&mut self.exist, exist);
        keep_new(&mut self.state, state);
        keep_new(&mut self.src_id, src_id);
        keep_new(&mut self.tar_id, tar_id);
    }
}

/// Dynamic extruder nozzle details.
///
/// Integrates both legacy abbreviated keys (standard platforms) and descriptive keys
/// (IDEX platforms) to provide unified schema matching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NozzleInfo {
    /// Extruder carriage index (0 = Right/Main, 1 = Left/Deputy), or on H2C, a packed rack
    /// slot: high nibble (bits 4–7) `1` flags a rack-stored spare nozzle, low nibble (bits
    /// 0–3) is the slot index within the rack — see [`NozzleInfo::is_rack_stored()`].
    pub id: u8,

    /// Nozzle orifice diameter in millimeters (e.g. 0.4).
    ///
    /// **Can be stale.** An empty hotend is still reported in `nozzle_info`, carrying the
    /// diameter of the nozzle it last held — measured on an idle H2C, where an unoccupied
    /// position read `diameter: 0.4` alongside `max_temp: 0` and serial `"N/A"`. Establish
    /// presence with [`NozzleInfo::is_installed()`] before trusting this.
    pub diameter: Option<f32>,

    /// Target maximum temperature (Standard Platform abbreviated representation). Each of the
    /// four values below has two wire spellings; read them through [`max_temp_c`](Self::max_temp_c),
    /// [`serial`](Self::serial), [`filament_colour`](Self::filament_colour) and
    /// [`filament_id`](Self::filament_id), which fall back from one to the other.
    pub tm: Option<u32>,

    /// Target maximum temperature (IDEX Platform verbose representation).
    pub max_temp: Option<u32>,

    /// Core physical nozzle composition or tool type designation.
    ///
    /// **Two vocabularies by generation.** Legacy printers report the nozzle *material* here
    /// (e.g. `"hardened_steel"`, `"stainless_steel"`); H2-generation printers report a *flow
    /// code* instead (`"HH"` = high flow, `"HS"` = standard, followed by a hardware-variant
    /// digit pair). Do not assume one and parse the other. The related but distinct
    /// `nozzle_id` on a K-profile entry uses the flow-code vocabulary only — see
    /// [`crate::diagnostics::KProfileEntry::nozzle_id`].
    #[serde(rename = "type")]
    pub nozzle_type: Option<String>,

    /// Normalized physical wear tracker value.
    ///
    /// A float: H2C, P2S and X2D send `0.0`, and BambuStudio stores it as `float m_wear`
    /// (`DevNozzleSystem.h`).
    pub wear: Option<f32>,

    /// Hotend manufacturer serial number (verbose IDEX platform representation).
    pub serial_number: Option<String>,

    /// Hotend manufacturer serial number (standard platform abbreviated representation).
    pub sn: Option<String>,

    /// Physical filament color hex code loaded into the extruder.
    pub filament_colour: Option<String>,

    /// Abbreviated filament color hex code.
    pub color_m: Option<String>,

    /// Filament preset calibration index.
    pub filament_id: Option<String>,

    /// Abbreviated filament preset calibration index.
    pub fila_id: Option<String>,

    /// Nozzle status bitmask.
    ///
    /// **Not a presence indicator.** It read `0` on every entry of an H2C's `nozzle_info`,
    /// occupied and empty alike — use [`NozzleInfo::is_installed()`] instead.
    #[serde(default)]
    pub stat: Option<u32>,

    /// Cumulative print time for this individual hotend.
    ///
    /// A wear/usage counter tied to the physical hotend rather than the position it sits in,
    /// which is what makes it meaningful on a rack machine where hotends are swapped between
    /// slots. Reported by H2C Vortek rack hotends; absent elsewhere — BambuStudio guards it with
    /// `if (njon.contains("p_t"))` and a `/*maybe not contains*/` note
    /// (`DevNozzleSystem.cpp`, parsing the same `device.nozzle` push this field comes
    /// from).
    ///
    /// **Units are seconds.** BambuStudio's nozzle-rack panel names the value `usedSeconds` and
    /// formats it as `usedSeconds / 3600` hours, falling back to `usedSeconds / 60` minutes
    /// under an hour and displaying `"0 h"` below a minute
    /// (`wgtDeviceNozzleRackUpdate.cpp`). ha-bambulab agrees independently, dividing by
    /// 3600 for an hours sensor (`definitions.py`).
    #[serde(default)]
    pub p_t: Option<u64>,
}

impl NozzleInfo {
    /// Maximum rated temperature in °C, from `max_temp` (IDEX spelling) or else `tm`.
    #[must_use]
    pub fn max_temp_c(&self) -> Option<u32> {
        self.max_temp.or(self.tm)
    }

    /// Hotend serial number, from `serial_number` (IDEX spelling) or else `sn`.
    #[must_use]
    pub fn serial(&self) -> Option<&str> {
        self.serial_number.as_deref().or(self.sn.as_deref())
    }

    /// Loaded filament colour hex code, from `filament_colour` or else `color_m`.
    #[must_use]
    pub fn filament_colour(&self) -> Option<&str> {
        self.filament_colour.as_deref().or(self.color_m.as_deref())
    }

    /// Filament preset id, from `filament_id` or else `fila_id`.
    #[must_use]
    pub fn filament_id(&self) -> Option<&str> {
        self.filament_id.as_deref().or(self.fila_id.as_deref())
    }

    /// Returns whether this entry is a rack-stored spare nozzle rather than an installed one.
    ///
    /// Confirmed directly against BambuStudio's source
    /// (`DevNozzleSystem.cpp`, `DevNozzleSystemParser::ParseV2_0`) — rack-stored spare
    /// nozzles are appended to the *same* `nozzle.info` array as installed ones, distinguished
    /// by `DevUtil::get_hex_bits(id, 1) == 1`. `get_hex_bits(num, pos, base=10)` extracts the
    /// 4-bit **nibble** at `pos*4` (`(num >> (pos*4)) & 0xF`), not a single bit — so this
    /// checks the *high* nibble (bits 4–7) of `id`, matching `reference/04_toolhead_thermal_
    /// motion.md`'s independently-documented H2C rack range of ids `16`-`21` (all of which
    /// have high nibble `1`; the low nibble `id & 0xF` is the rack slot index). Reachable on
    /// real hardware: H2C ("2 Slots, up to 7 active nozzles" per `MODEL_MATRIX.csv`) is a
    /// currently-modeled printer with existing rack-aware code elsewhere
    /// (`src/client/thermal.rs`'s H2C nozzle-ID validation, `src/quirks/mod.rs`).
    pub fn is_rack_stored(&self) -> bool {
        (self.id >> 4) & 0xF == 1
    }

    /// Returns whether a hotend is physically mounted in this position.
    ///
    /// An empty hotend is **not** omitted from `nozzle_info` — it is still reported, keeping
    /// the [`diameter`](Self::diameter) of whatever it last held. Measured on an idle H2C, an
    /// unoccupied position read `diameter: 0.4`, `max_temp: 0`, serial `"N/A"`. So presence has
    /// to be *stated*, and neither field states it alone:
    ///
    /// * the serial must be the firmware's explicit `"N/A"` sentinel, **and**
    /// * the temperature rating must be absent or zero.
    ///
    /// Either check on its own gives a wrong answer on some model, because a firmware that
    /// reports neither field normalizes to the same shape as an empty hotend. Requiring both
    /// means a not-reported entry reads as installed, which is the safe direction: it defers to
    /// whatever the caller does with a present-but-unknown hotend rather than silently hiding
    /// one.
    ///
    /// [`stat`](Self::stat) is not usable for this — it read `0` on every entry, occupied and
    /// empty alike. An empty **rack dock** is a different case: it is absent from the payload
    /// entirely, so an id in `16..=21` already implies a nozzle is in it (see
    /// [`is_rack_stored()`](Self::is_rack_stored)).
    pub fn is_installed(&self) -> bool {
        let serial_says_empty = self.serial().is_some_and(|s| s.eq_ignore_ascii_case("N/A"));
        let rating_says_empty = self.max_temp_c().unwrap_or(0) == 0;

        !(serial_says_empty && rating_says_empty)
    }
}

/// IDEX extruder collection from `device.extruder` [REF-THER-DECODE §Dual-Extruder].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtruderCollection {
    /// Per-extruder thermal and routing entries (id 0 = right/main, id 1 = left/deputy).
    ///
    /// `Option<Vec<_>>` for the same absent-vs-present-empty reason as `NozzleCollection.info`
    /// — see its doc comment.
    #[serde(default)]
    pub info: Option<Vec<ExtruderInfo>>,

    /// Bitmask: low 4 bits = extruder count, bits 4–7 = active extruder index.
    pub state: Option<u32>,
}

impl ExtruderCollection {
    /// Returns the active extruder index extracted from the `state` bitmask.
    pub fn active_extruder_index(&self) -> u8 {
        self.state.map_or(0, |s| ((s >> 4) & 0xF) as u8)
    }

    /// Returns the extruder count extracted from the `state` bitmask.
    pub fn extruder_count(&self) -> u8 {
        self.state.map_or(0, |s| (s & 0xF) as u8)
    }
}

impl Mergeable for ExtruderCollection {
    /// Merges a freshly-parsed `ExtruderCollection` into `self` field-by-field.
    ///
    /// Confirmed via `pybambu` and `bambuddy` (see `DeviceTelemetry::merge_from`) —
    /// `device.extruder.info` can be absent from a push while sibling `device.extruder`
    /// fields change, and must not be treated as "extruder info cleared." `info` is
    /// now `Option<Vec<_>>` (see its doc comment) so a present-but-empty push actually clears
    /// — wholesale replace, matching BambuStudio's own `ExtderSystemParser::ParseV2_0`
    /// (`system->m_extders.clear()` + full rebuild).
    fn merge_from(&mut self, incoming: &Self) {
        let Self { info, state } = incoming;
        keep_new(&mut self.info, info);
        keep_new(&mut self.state, state);
    }
}

/// Per-extruder thermal and routing state for IDEX platforms.
///
/// The `temp` field uses the same composite packing as `chamber_temper`:
/// values > 500 encode `(target << 16) | actual`, values <= 500 are direct actual temps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtruderInfo {
    /// Extruder carriage index (0 = right/main, 1 = left/deputy).
    pub id: u8,

    /// Composite-packed temperature; decode with [`temperatures()`](Self::temperatures).
    pub temp: Option<u32>,

    /// Current AMS slot routing (confirmed against BambuStudio's `DevExterSystemParser::ParseV2_0`, `DevExtruderSystem.cpp`): low 8 bits (0–7) = slot_id, next 8 bits (8–15) = ams_id. Sentinel `0xFFFF` on a single-extruder system means unmapped.
    pub snow: Option<u32>,

    /// Previous AMS slot routing. Same 8/8 (slot_id/ams_id) bit split as `snow`.
    pub spre: Option<u32>,

    /// Target AMS slot routing. Same 8/8 (slot_id/ams_id) bit split as `snow`.
    pub star: Option<u32>,

    /// Current head routing index.
    pub hnow: Option<u8>,

    /// Previous head routing index.
    pub hpre: Option<u8>,

    /// Target head routing index.
    pub htar: Option<u8>,

    /// Status bitmask.
    pub stat: Option<u32>,

    /// Info bitmask.
    ///
    /// Three bits are known, decoded by BambuStudio's `DevExtruderSystem.cpp` via
    /// `DevUtil::get_flag_bits(info, N)` (which reads a single bit at position `N`, its `count`
    /// defaulting to 1):
    ///
    /// | Bit | Mask | Meaning |
    /// |---|---|---|
    /// | 1 | `0b0010` | The extruder holds filament |
    /// | 2 | `0b0100` | The buffer holds filament |
    /// | 3 | `0b1000` | A nozzle is fitted |
    ///
    /// Read out of BambuStudio's parser rather than from a wire capture; bit 1 is independently
    /// corroborated by bambuddy's `ExtruderSlot`, which computes `has_filament` as
    /// `bool(flags & 0b10)`. Bits 0 and 4+ have no recorded meaning.
    pub info: Option<u32>,

    /// Filament backup groups, one bitmask per group — not slot indices.
    ///
    /// Each set bit is a member tray in the `tray_exist_bits` layout (bits 0-15 standard AMS
    /// `ams_id*4 + slot`, 16-23 AMS-HT 128-135, 24-27 an A2L's AMS Lite); a slot's backups are
    /// the other members of its group. See `reference/05_materials_ams.md`.
    #[serde(default)]
    pub filam_bak: Vec<u32>,

    /// Z-axis offset compensation (X2D).
    pub z_bias: Option<f64>,
}

impl ExtruderInfo {
    /// Unpacks the composite `temp`; `None` when it is absent.
    #[must_use]
    pub fn temperatures(&self) -> Option<HeaterTemps> {
        Some(unpack_temperature(f64::from(self.temp?)))
    }

    /// Decodes an AMS-routing field (`snow`/`spre`/`star`) into `(ams_id, slot_id)`.
    /// Confirmed against BambuStudio's `DevExterSystemParser::ParseV2_0`
    /// (`DevExtruderSystem.cpp`): low 8 bits = slot_id, next 8 bits = ams_id.
    ///
    /// The sentinel `0xFFFF` decodes to `None` unconditionally, on every extruder count —
    /// deliberately, not an oversight. BambuStudio's own parser only special-cases `0xffff`
    /// when `m_total_extder_count == 1` (`DevExtruderSystem.cpp`); on a 2-extruder
    /// (IDEX) system a raw `0xffff` there falls through to the normal decode instead
    /// (`ams_id=255, slot_id=255`). bambuddy deliberately diverges from that literal gating
    /// and matches this crate's unconditional treatment, with a stated rationale
    /// (`bambu_mqtt.py`): "0xFFFF decodes to AMS 255 slot 255 and slot 255 is not a
    /// real slot on any machine, so treating it as empty everywhere is strictly safer than
    /// reading it as the external spool." Re-litigated without new evidence in the
    /// 2026-09-08 telemetry review sweep, same conclusion — don't reopen without a wire
    /// capture showing a genuine `ams_id=255, slot_id=255` combo in the wild.
    ///
    /// The decoded `ams_id` goes through
    /// [`normalize_ams_unit_id`](crate::ams::normalize_ams_unit_id) before it is returned, so
    /// an AMS Lite **attached to an A2L** — which reports physical id 16 rather than the 0 the
    /// same unit uses as an A1's only AMS, since on an A2L it coexists with up to four
    /// shared-pool units already holding ids 0-3 — arrives as the 6 the rest of the crate
    /// addresses it by. Without it `resolve_global_tray_id(16, slot)` — which accepts only
    /// 0-3, 6, 128-135 and 254/255 — fell through to `None`, and `printing_tray_global_id()`
    /// reported "no active tray" on an A2L printing from that unit.
    ///
    /// That the `snow`/`spre`/`star` byte shares an id space with the AMS unit's own `id`
    /// field is confirmed by both upstreams: BambuStudio compares the two directly and
    /// unnormalized (`DevAms::GetCurrentExtruderId`, `DevFilaSystem.cpp`, testing
    /// `extruder.GetSlotNow().ams_id == m_ams_id`, where `m_ams_id` is the raw reported `id`
    /// from `DevFilaSystem.cpp`), and bambuddy records that id as 16 for this combination
    /// (`_normalize_a2l_am_units`, `services/bambu_mqtt.py`).
    ///
    /// Neither upstream normalizes this decode path. BambuStudio does not need to: it gives
    /// this combination its own unit type — `AMS_LITE_MIXED = 5`, commented "AMS-Lite for N9",
    /// N9 being the A2L's dev token, read straight from the unit's own `info` type nibble —
    /// and computes `24 + slot_id` for it, ignoring `ams_id` entirely
    /// (`DevFilaSystem.cpp`). That is the same global tray id this crate reaches through
    /// `6 * 4 + slot`, so the two agree on the answer while disagreeing on the route.
    ///
    /// Normalizing here rather than widening `resolve_global_tray_id` keeps the promise
    /// [`normalize_ams_unit_id`](crate::ams::normalize_ams_unit_id) already makes — that the
    /// inbound telemetry boundary owns this remap — instead of adding a fourth site that
    /// re-derives the id ranges by hand.
    fn decode_ams_slot_field(raw: Option<u32>) -> Option<(u8, u8)> {
        let raw = raw?;
        if raw == 0xFFFF {
            return None;
        }
        let slot_id = (raw & 0xFF) as u8;
        let ams_id = crate::ams::ids::normalize_ams_unit_id(((raw >> 8) & 0xFF) as u8);
        Some((ams_id, slot_id))
    }

    /// Currently routed `(ams_id, slot_id)`, decoded from `snow` — the preferred source for
    /// resolving which physical tray is feeding this extruder right now, confirmed
    /// against BambuStudio's `ExtderSystemParser::ParseV2_0` (`DevExtruderSystem.cpp`), which
    /// decodes `snow` directly with no extruder-map inversion needed.
    pub fn current_ams_slot(&self) -> Option<(u8, u8)> {
        Self::decode_ams_slot_field(self.snow)
    }

    /// Previously routed `(ams_id, slot_id)`, decoded from `spre`. See
    /// [`ExtruderInfo::current_ams_slot`]'s doc comment for the shared bit layout.
    pub fn previous_ams_slot(&self) -> Option<(u8, u8)> {
        Self::decode_ams_slot_field(self.spre)
    }

    /// Target `(ams_id, slot_id)` for an in-progress filament change, decoded from `star`. See
    /// [`ExtruderInfo::current_ams_slot`]'s doc comment for the shared bit layout.
    pub fn target_ams_slot(&self) -> Option<(u8, u8)> {
        Self::decode_ams_slot_field(self.star)
    }
}

/// Climate parts collection nested within `device` parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AirductCollection {
    /// Array of active climate routing nodes (heaters, dampers, supplementary fans) [REF-CLIM-FANS].
    ///
    /// `Option<Vec<_>>` for the same absent-vs-present-empty reason as `NozzleCollection.info`
    /// — see its doc comment.
    #[serde(default)]
    pub parts: Option<Vec<AirductPart>>,

    /// Currently active airduct damper mode (0=cooling, 1=heating, 2=laser).
    #[serde(rename = "modeCur")]
    pub mode_cur: Option<i32>,

    /// List of airduct modes available on this model.
    ///
    /// `Option<Vec<_>>` for the same absent-vs-present-empty reason as `NozzleCollection.info`
    /// — see its doc comment.
    #[serde(rename = "modeList", default)]
    pub mode_list: Option<Vec<AirductModeListEntry>>,
}

impl AirductCollection {
    /// The speed of the fan reported as part `id`, as a percentage (0-100).
    ///
    /// Two wire shapes, both real, and the fix for each broke the other once (#31, then #184),
    /// so order matters:
    ///
    /// 1. A negative state is a firmware sentinel for "off/unknown" and reads `None`. It must be
    ///    rejected *before* the mask, since `-1 & 0xFF == 255`, which would clamp to a bogus 100%.
    /// 2. A non-negative state may be bit-packed, with the percentage in the low byte and flags
    ///    above it. BambuStudio's `DevFan::ParseV3_0` applies `get_flag_bits(state, 0, 8)`
    ///    unconditionally to every airduct part, and bambuddy independently does the same
    ///    `int(part["state"]) & 0xFF`. Without the mask a packed `306` clamps to 100 instead of
    ///    decoding to its real 50.
    #[must_use]
    pub fn part_percent(&self, id: u32) -> Option<u8> {
        let state = self.parts.as_deref()?.iter().find(|p| p.id == id)?.state?;
        if state < 0 {
            return None;
        }
        Some((state & 0xFF).clamp(0, 100) as u8)
    }
}

impl Mergeable for AirductCollection {
    /// Merges a freshly-parsed `AirductCollection` into `self` field-by-field.
    ///
    /// Confirmed via `pybambu` and `bambuddy` (see `DeviceTelemetry::merge_from`) —
    /// `device.airduct.parts`/`modeCur`/`modeList` can each independently be absent from a
    /// push, and must not be treated as "cleared." `parts`/`mode_list` are now
    /// `Option<Vec<_>>` (see their doc comments) so a present-but-empty push actually clears.
    ///
    /// `parts` is merged per-id (upsert into the existing `Vec` by id) rather than replaced
    /// wholesale — unlike BambuStudio, this crate doesn't perform a full JSON-diff
    /// reconstruction upstream of this call, so a present-but-*partial* frame (a subset of
    /// part ids, not the same as present-but-empty) must not wipe previously-known parts the
    /// incoming frame simply didn't repeat. A present-but-empty `parts` array is still treated
    /// as an explicit clear, matching BambuStudio's `json_diff::restore_objects` (which fully
    /// reconstructs `device.airduct` before `DevFan.cpp`'s own parser unconditionally clears
    /// and rebuilds `parts`/`modeList` from that already-correct snapshot) for that one case.
    fn merge_from(&mut self, incoming: &Self) {
        let Self {
            parts,
            mode_cur,
            mode_list,
        } = incoming;
        if let Some(parts) = parts {
            if parts.is_empty() {
                self.parts = Some(Vec::new());
            } else {
                let existing = self.parts.get_or_insert_with(Vec::new);
                for incoming_part in parts {
                    match existing.iter_mut().find(|p| p.id == incoming_part.id) {
                        Some(existing_part) => *existing_part = incoming_part.clone(),
                        None => existing.push(incoming_part.clone()),
                    }
                }
            }
        }
        keep_new(&mut self.mode_cur, mode_cur);
        keep_new(&mut self.mode_list, mode_list);
    }
}

/// Entry in the airduct mode availability list reported by the printer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AirductModeListEntry {
    /// Mode identifier (0=cooling, 1=heating, 2=laser).
    #[serde(rename = "modeId")]
    pub mode_id: i32,
}

/// Represents an individual auxiliary routing component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AirductPart {
    /// Part index matching hardware configurations (e.g., `160` for the second left-side
    /// auxiliary fan on X2D/P2S — despite the wire port number suggesting a "right" fan).
    pub id: u32,

    /// The active operating speed percentage (`0` to `100`) or damper direction flag.
    pub state: Option<i32>,
}
