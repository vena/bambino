//! Device-level hardware telemetry (extruders, nozzles, bed, fans, airduct, CTC, cameras).

#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use super::diagnostics::CtcTelemetry;

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
    /// BUG-054: a fixture payload carries the identical value in both fields, and both
    /// pybambu (`models.py`, reads only `device.bed.info.temp`) and bambuddy independently
    /// never consult this field either. Parsed for wire-format completeness only —
    /// `decode_bed_temperatures()` deliberately does not read it.
    #[serde(default)]
    pub bed_temp: Option<u32>,
}

impl DeviceTelemetry {
    /// Merges a freshly-parsed `DeviceTelemetry` into `self` field-by-field, instead of
    /// replacing `self` wholesale.
    ///
    /// BUG-093: same shape as `AmsStatusReport::merge_from` (BUG-091) one struct up — a
    /// `device` push touching only one sub-object (e.g. `ctc`) has every other field simply
    /// absent from that message, not explicitly cleared. Replacing `self` wholesale on any
    /// `Some(_)` push wiped the other cached sub-objects (`nozzle`, `extruder`, `airduct`,
    /// `bed`, `ext_tool`) back to `None`.
    ///
    /// BUG-094: recurses into `nozzle`/`extruder`/`airduct` rather than replacing them
    /// wholesale when both sides have `Some(_)` — confirmed via `pybambu` and `bambuddy`
    /// (see each collection's own `merge_from`) that `device.nozzle.info`,
    /// `device.extruder.info`, and `device.airduct.modeCur`/`modeList`/`parts` can each be
    /// absent independent of their parent sub-object arriving. `ctc`/`bed`/`ext_tool` are not
    /// recursed into — no evidence at that finer grain from either source.
    pub(crate) fn merge_from(&mut self, incoming: &DeviceTelemetry) {
        match (&mut self.nozzle, &incoming.nozzle) {
            (Some(cached), Some(new)) => cached.merge_from(new),
            (None, Some(new)) => self.nozzle = Some(new.clone()),
            _ => {}
        }
        match (&mut self.extruder, &incoming.extruder) {
            (Some(cached), Some(new)) => cached.merge_from(new),
            (None, Some(new)) => self.extruder = Some(new.clone()),
            _ => {}
        }
        match (&mut self.airduct, &incoming.airduct) {
            (Some(cached), Some(new)) => cached.merge_from(new),
            (None, Some(new)) => self.airduct = Some(new.clone()),
            _ => {}
        }
        if incoming.ctc.is_some() {
            self.ctc = incoming.ctc.clone();
        }
        if incoming.bed.is_some() {
            self.bed = incoming.bed.clone();
        }
        if incoming.ext_tool.is_some() {
            self.ext_tool = incoming.ext_tool.clone();
        }
        if incoming.fire_ext.is_some() {
            self.fire_ext = incoming.fire_ext.clone();
        }
        if incoming.bed_temp.is_some() {
            self.bed_temp = incoming.bed_temp;
        }
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

/// Bed info segment with composite-packed temperature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BedInfo {
    /// Composite-packed bed temperature [REF-THER-DECODE].
    #[serde(default)]
    pub temp: Option<u32>,
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

/// Wrap block holding nozzle characteristics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NozzleCollection {
    /// Polymorphic array representing active carriages and tool configurations.
    #[serde(default)]
    pub info: Vec<NozzleInfo>,

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

impl NozzleCollection {
    /// Merges a freshly-parsed `NozzleCollection` into `self` field-by-field.
    ///
    /// BUG-094: confirmed via `pybambu` and `bambuddy` (see `DeviceTelemetry::merge_from`) —
    /// `device.nozzle.info` can be absent from a push while sibling `device.nozzle` fields
    /// change, and must not be treated as "nozzle info cleared."
    pub(crate) fn merge_from(&mut self, incoming: &NozzleCollection) {
        if !incoming.info.is_empty() {
            self.info = incoming.info.clone();
        }
        if incoming.exist.is_some() {
            self.exist = incoming.exist;
        }
        if incoming.state.is_some() {
            self.state = incoming.state;
        }
        if incoming.src_id.is_some() {
            self.src_id = incoming.src_id;
        }
        if incoming.tar_id.is_some() {
            self.tar_id = incoming.tar_id;
        }
    }
}

/// Dynamic extruder nozzle details.
///
/// Integrates both legacy abbreviated keys (standard platforms) and descriptive keys
/// (IDEX platforms) to provide unified schema matching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NozzleInfo {
    /// Extruder carriage index (0 = Right/Main, 1 = Left/Deputy) or storage rack index.
    pub id: u8,

    /// Nozzle orifice diameter in millimeters (e.g. 0.4).
    pub diameter: Option<f32>,

    /// Target maximum temperature (Standard Platform abbreviated representation).
    pub tm: Option<u32>,

    /// Target maximum temperature (IDEX Platform verbose representation).
    pub max_temp: Option<u32>,

    /// Core physical nozzle composition or tool type designation.
    #[serde(rename = "type")]
    pub nozzle_type: Option<String>,

    /// Normalized physical wear tracker value.
    pub wear: Option<u32>,

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
    #[serde(default)]
    pub stat: Option<u32>,
}

/// IDEX extruder collection from `device.extruder` [REF-THER-DECODE §Dual-Extruder].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtruderCollection {
    /// Per-extruder thermal and routing entries (id 0 = right/main, id 1 = left/deputy).
    #[serde(default)]
    pub info: Vec<ExtruderInfo>,

    /// Bitmask: low 4 bits = extruder count, bits 4–7 = active extruder index.
    pub state: Option<u32>,
}

impl ExtruderCollection {
    /// Returns the active extruder index extracted from the `state` bitmask.
    pub fn active_extruder_index(&self) -> u8 {
        self.state.map(|s| ((s >> 4) & 0xF) as u8).unwrap_or(0)
    }

    /// Returns the extruder count extracted from the `state` bitmask.
    pub fn extruder_count(&self) -> u8 {
        self.state.map(|s| (s & 0xF) as u8).unwrap_or(0)
    }

    /// Merges a freshly-parsed `ExtruderCollection` into `self` field-by-field.
    ///
    /// BUG-094: confirmed via `pybambu` and `bambuddy` (see `DeviceTelemetry::merge_from`) —
    /// `device.extruder.info` can be absent from a push while sibling `device.extruder`
    /// fields change, and must not be treated as "extruder info cleared."
    pub(crate) fn merge_from(&mut self, incoming: &ExtruderCollection) {
        if !incoming.info.is_empty() {
            self.info = incoming.info.clone();
        }
        if incoming.state.is_some() {
            self.state = incoming.state;
        }
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

    /// Composite-packed temperature (use `unpack_temperature()` to decode).
    pub temp: Option<u32>,

    /// Current AMS slot routing (low 4 bits = tray index, upper bits = AMS unit index).
    pub snow: Option<u32>,

    /// Previous AMS slot routing.
    pub spre: Option<u32>,

    /// Target AMS slot routing.
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
    pub info: Option<u32>,

    /// Filament backup slot indices.
    #[serde(default)]
    pub filam_bak: Vec<u32>,

    /// Z-axis offset compensation (X2D).
    pub z_bias: Option<f64>,
}

impl ExtruderInfo {
    /// Unpacks the composite temperature into (actual, target) degrees Celsius.
    pub fn temperatures(&self) -> (u16, u16) {
        self.temp
            .map(|t| super::report::PrinterTelemetry::unpack_temperature(t as f64))
            .unwrap_or((0, 0))
    }
}

/// Climate parts collection nested within `device` parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AirductCollection {
    /// Array of active climate routing nodes (heaters, dampers, supplementary fans) [REF-CLIM-FANS].
    #[serde(default)]
    pub parts: Vec<AirductPart>,

    /// Currently active airduct damper mode (0=cooling, 1=heating, 2=laser).
    #[serde(rename = "modeCur")]
    pub mode_cur: Option<i32>,

    /// List of airduct modes available on this model.
    #[serde(rename = "modeList", default)]
    pub mode_list: Vec<AirductModeListEntry>,
}

impl AirductCollection {
    /// Merges a freshly-parsed `AirductCollection` into `self` field-by-field.
    ///
    /// BUG-094: confirmed via `pybambu` and `bambuddy` (see `DeviceTelemetry::merge_from`) —
    /// `device.airduct.parts`/`modeCur`/`modeList` can each independently be absent from a
    /// push, and must not be treated as "cleared."
    pub(crate) fn merge_from(&mut self, incoming: &AirductCollection) {
        if !incoming.parts.is_empty() {
            self.parts = incoming.parts.clone();
        }
        if incoming.mode_cur.is_some() {
            self.mode_cur = incoming.mode_cur;
        }
        if !incoming.mode_list.is_empty() {
            self.mode_list = incoming.mode_list.clone();
        }
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
    /// Part index matching hardware configurations (e.g., `160` for the right auxiliary fan).
    pub id: u32,

    /// The active operating speed percentage ($0$ to $100$) or damper direction flag.
    pub state: Option<i32>,
}
