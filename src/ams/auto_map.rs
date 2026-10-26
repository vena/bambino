//! # Deriving an AMS mapping from a plate's filaments and the loaded trays
//!
//! A port of BambuStudio's automatic AMS mapping: `DevMappingUtil::ams_filament_mapping`
//! (`DeviceCore/DevMapping.cpp`) as `SelectMachineDialog::do_ams_mapping` (`SelectMachine.cpp`)
//! calls it with `use_ams = true`. It is what a consumer that doesn't ask the user for a
//! mapping needs; see `reference/05_materials_ams.md`, "Choosing a Mapping When the User Is Not
//! Asked".
//!
//! [`derive_ams_mapping`] takes the plate's filaments (from
//! [`read_plate_filaments`](crate::ftps::project::read_plate_filaments), or built by the caller)
//! and the cached AMS telemetry, and does no I/O. Its [`AmsMappingResult`] holds the
//! `ams_mapping2` array [`PrintJobConfig::with_ams_mapping2`](crate::mqtt::PrintJobConfig::with_ams_mapping2)
//! takes and, per filament, which tray it matched, so a consumer can say "filament 3 has no
//! loaded match" instead of printing wrong.
//!
//! # The match
//!
//! For each filament and each loaded tray BambuStudio computes a distance, and picks the
//! lowest. A tray of a different material is never picked (`type` is a hard condition). For the
//! same material the distance is the colour distance (0 to 1000, CIELAB ΔE76 combined with the
//! alpha difference; see [`color_distance`]) plus 0 when the preset (`setting_id`) matches, 10000
//! when only the filament class (`filament_id`, the file's `tray_info_idx` against the tray's)
//! matches, and 20000 otherwise. So preset beats class beats colour.
//!
//! **A file's filament has no `setting_id`**, only the `tray_info_idx` that BambuStudio calls
//! `filament_id`, so for a file the preset branch never fires and a match is class, then colour.
//! The code keeps the branch because the internal filament record carries a `setting_id`; the
//! public entry point leaves it empty.
//!
//! The selection is greedy: one round per filament, each picking the lowest-distance pair among
//! the filaments not yet mapped and the trays not yet used. A used tray can be picked again
//! only at distance under 0.0001, or, when no unused tray matched any remaining filament, at
//! any distance (BambuStudio's "retry to mapping to used tray"), so two filaments of one
//! material share a single loaded tray rather than going unmapped. A tie prefers an exact
//! `filament_id` match, then the tray with less filament left among exact matches
//! (`_prefer_tray_by_remain`).
//!
//! # Differences from BambuStudio
//!
//! - **Index versus key.** BambuStudio's loop reads `tray_filaments[picked_tar_idx]` where
//!   `tray_filaments` is a `std::map` keyed by the tray's flat index and `picked_tar_idx` is a
//!   column position. Whenever the two differ (any unit but the first, any gap), `operator[]`
//!   inserts or reads the wrong tray in the tie-break. This port compares the tray the column
//!   names.
//! - **Retry order.** BambuStudio tries the used-tray retry for a filament as soon as that
//!   filament finds nothing unused, before later filaments are looked at. This port finishes the
//!   unused-tray pass over every remaining filament first, which is what its comments describe.
//! - **Empty ids never tie-break.** BambuStudio's tie-break compares `filament_id` strings with
//!   `==`, so two empty ids count as an exact match. Here an exact match needs a non-empty id,
//!   as the class offset already does.
//! - **A loaded tray** is one [`AmsTray::is_loaded`] accepts and that names a material, since the
//!   cache holds stale fields for an emptied tray (BambuStudio blanks the type itself).
//! - **Single-nozzle printers ignore a unit's extruder binding.** BambuStudio filters units by
//!   binding on every printer; the binding only means something with two extruders.
//! - **No filament switch signal.** For a unit reporting `0xE` ("not fixed"), BambuStudio uses
//!   both extruders only when a Filament Track Switch is installed and the unit's
//!   `bind_switch_in` is a valid inlet, and none otherwise (`ParseAmsInfo`, `DevFilaSystem.cpp`).
//!   bambino doesn't model the switch's own telemetry (bambuddy detects it from the presence of
//!   `device.fila_switch`), so it checks the inlet alone: both extruders with one, none without.
//!
//! # Printers with two extruders
//!
//! Filaments are split by the file's `filament_maps` (`1` left/deputy, `2` right/main) and each
//! side is matched on its own against the AMS units bound to that extruder, plus that side's
//! external spool only when no unit is bound to it. A filament with no usable `filament_maps`
//! value is left unmatched. A unit reachable from both extruders (a Filament Track Switch)
//! is a candidate on both sides, and, as in BambuStudio, the two sides don't see each other's
//! picks. On a single-nozzle printer the external spool is never a candidate.
//!
//! # Using the result
//!
//! A filament with no match is [`AmsMapping2Entry::UNMAPPED`] in the array. Sending a job that
//! uses such a filament fails with an AMS mapping error on the printer; whether to send
//! anyway, ask the user, or stop is the caller's decision, which is why
//! [`AmsMappingResult::unmatched`] exists.

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};
#[cfg(not(feature = "std"))]
use alloc::vec;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::ids::{
    AMS_EXTERNAL_SPOOL_DEPUTY_ID, AMS_EXTERNAL_SPOOL_MAIN_ID, AMS_SLOTS_PER_UNIT, is_ams_ht_id,
    is_ams_lite_on_a2l_id, is_bus_unit_id, is_standard_id, is_unit_slot,
};
use super::mapping::{AMS_MAX_PROJECT_FILAMENTS, AmsMapping2Entry, MaterialSource};
use crate::ftps::project::{PlateFilament, PlateFilaments};
use crate::types::telemetry::{AmsStatusReport, AmsTray, AmsUnit, VirtualTray};

/// Smallest colour distance, BambuStudio's `kColorDistanceMin`.
const COLOR_DISTANCE_MIN: f32 = 0.0;
/// Largest colour distance, BambuStudio's `kColorDistanceMax`: above any real ΔE76 (about 260).
const COLOR_DISTANCE_MAX: f32 = 1000.0;
/// Added when only the filament class matches, BambuStudio's `kClassDistanceOffset`.
const CLASS_DISTANCE_OFFSET: f32 = 10_000.0;
/// Added when only the material matches, BambuStudio's `kTypeDistanceOffset`.
const TYPE_DISTANCE_OFFSET: f32 = 20_000.0;
/// A used tray is a candidate again only below this distance.
const REUSE_DISTANCE: f32 = 0.0001;
/// A `u8` channel's range, to scale it into `0.0..=1.0`.
const CHANNEL_MAX: f32 = 255.0;
/// The weight of the alpha difference, which BambuStudio scales into CIE L*'s `0..=100`.
const ALPHA_SCALE: f32 = 100.0;
/// Opaque white: what BambuStudio's `DevAmsTray::decode_color` answers for an empty or invalid colour.
const WHITE: [u8; 4] = [255, 255, 255, 255];
/// A tray's flat index base for an A2L-attached AMS Lite, BambuStudio's `AMS_LITE_MIXED_TRAY_INDEX_OFFSET`.
const AMS_LITE_ORDER_BASE: u16 = 24;
/// `filament_maps` value for the right (main) extruder.
const FILAMENT_MAP_MAIN: u8 = 2;
/// `filament_maps` value for the left (deputy) extruder.
const FILAMENT_MAP_DEPUTY: u8 = 1;
/// The unit `info` extruder value for the main extruder.
const EXTRUDER_MAIN: u8 = 0;
/// The unit `info` extruder value for the deputy extruder.
const EXTRUDER_DEPUTY: u8 = 1;

/// Where one filament landed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FilamentMatch {
    /// The project filament index, 1-based, as [`PlateFilament::id`].
    pub filament_id: u32,
    /// The tray it matched, or `None` when nothing loaded matched.
    pub source: Option<MaterialSource>,
    /// BambuStudio's distance for the pair, `None` when unmatched.
    ///
    /// Under 1000 is a preset match and the colour distance; 10000 to 11000 a class match plus
    /// colour; 20000 and above a same-material tray of another class. Read it to decide whether
    /// a match is close enough to print.
    pub distance: Option<f32>,
}

/// The mapping [`derive_ams_mapping`] found and what each filament matched.
#[derive(Debug, Clone, PartialEq)]
pub struct AmsMappingResult {
    /// The `ams_mapping2` array: one entry per project filament index up to the highest the
    /// plate uses, [`AmsMapping2Entry::UNMAPPED`] for an index the plate doesn't use and for a
    /// filament nothing matched. Pass it to
    /// [`PrintJobConfig::with_ams_mapping2`](crate::mqtt::PrintJobConfig::with_ams_mapping2).
    pub mapping2: Vec<AmsMapping2Entry>,
    /// One record per filament given, in the order given.
    pub matches: Vec<FilamentMatch>,
}

impl AmsMappingResult {
    /// The filaments nothing loaded matched.
    ///
    /// A job started with one of these in `mapping2` fails with an AMS mapping error. Report it,
    /// ask the user for a mapping, or stop: the crate does not choose.
    pub fn unmatched(&self) -> impl Iterator<Item = &FilamentMatch> {
        self.matches.iter().filter(|m| m.source.is_none())
    }

    /// Whether every filament matched a tray.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.unmatched().next().is_none()
    }
}

/// Derives an `ams_mapping2` for `filaments` from the AMS telemetry in `ams` and the external spools in `external_spools`.
///
/// `ams` is [`PrinterClient::ams`](crate::client::PrinterClient::ams) and `external_spools` is
/// [`vir_slot`](crate::client::PrinterClient::vir_slot) (empty on a single-nozzle printer, which
/// never maps to its external spool). `dual_nozzle` is whether the printer has more than one
/// nozzle, `quirks().physical_nozzle_count() > 1`. `None` for `ams`, or no loaded tray, leaves
/// every filament unmatched; it is not an error.
///
/// Prefer [`PrinterClient::derive_ams_mapping`](crate::client::PrinterClient::derive_ams_mapping),
/// which supplies all three. See the module doc for the match and where it differs from
/// BambuStudio. A filament whose `id` is 0 or above 32 (BambuStudio's project limit) is left
/// unmatched and out of the array.
#[must_use]
pub fn derive_ams_mapping(
    filaments: &PlateFilaments,
    ams: Option<&AmsStatusReport>,
    external_spools: &[VirtualTray],
    dual_nozzle: bool,
) -> AmsMappingResult {
    let wanted: Vec<Wanted<'_>> = filaments.filaments.iter().map(Wanted::from_file).collect();
    let mut picks: Vec<Option<(MaterialSource, f32)>> = vec![None; wanted.len()];

    let valid = |w: &Wanted<'_>| (1..=AMS_MAX_PROJECT_FILAMENTS as u32).contains(&w.id);
    let mut run = |side: &[usize], candidates: &[Candidate]| {
        let group: Vec<&Wanted<'_>> = side.iter().map(|&i| &wanted[i]).collect();
        for (&i, pick) in side.iter().zip(match_group(&group, candidates)) {
            picks[i] = pick.map(|(tray, distance)| (candidates[tray].source, distance));
        }
    };

    let usable: Vec<usize> = (0..wanted.len()).filter(|&i| valid(&wanted[i])).collect();
    if dual_nozzle {
        for (map_value, extruder) in [
            (FILAMENT_MAP_DEPUTY, EXTRUDER_DEPUTY),
            (FILAMENT_MAP_MAIN, EXTRUDER_MAIN),
        ] {
            let side: Vec<usize> = usable
                .iter()
                .copied()
                .filter(|&i| filaments.extruder_of(wanted[i].id) == Some(map_value))
                .collect();
            run(&side, &extruder_candidates(ams, external_spools, extruder));
        }
    } else {
        run(&usable, &single_nozzle_candidates(ams));
    }

    let mut allocations = Vec::new();
    let mut matches = Vec::new();
    for (w, pick) in wanted.iter().zip(&picks) {
        matches.push(FilamentMatch {
            filament_id: w.id,
            source: pick.map(|(source, _)| source),
            distance: pick.map(|(_, distance)| distance),
        });
        if valid(w) {
            let source = pick.map_or(MaterialSource::Unmapped, |(source, _)| source);
            allocations.push((w.id as usize, source));
        } else {
            log::warn!(
                "derive_ams_mapping: filament id {} is outside 1..={AMS_MAX_PROJECT_FILAMENTS}, left unmatched",
                w.id
            );
        }
    }
    AmsMappingResult {
        mapping2: super::mapping::build_ams_mapping2(&allocations),
        matches,
    }
}

/// A filament the slicer asked for, in the terms the match compares.
struct Wanted<'a> {
    id: u32,
    filament_type: &'a str,
    color: [u8; 4],
    /// BambuStudio's `filament_id`: the file's `tray_info_idx`.
    filament_id: &'a str,
    /// BambuStudio's `setting_id`: the preset, which a file does not carry.
    setting_id: &'a str,
}

impl<'a> Wanted<'a> {
    fn from_file(filament: &'a PlateFilament) -> Self {
        Self {
            id: filament.id,
            filament_type: &filament.filament_type,
            color: parse_color(&filament.color),
            filament_id: &filament.tray_info_idx,
            setting_id: "",
        }
    }
}

/// A loaded tray a filament can be matched to.
struct Candidate {
    /// BambuStudio's flat tray index, which orders the candidates and so breaks exact ties.
    order: u16,
    source: MaterialSource,
    filament_type: String,
    color: [u8; 4],
    filament_id: String,
    setting_id: String,
    /// Percent remaining, `-1` when the firmware reports none.
    remain: i32,
}

/// The candidates of a single-nozzle printer: every loaded AMS tray, in tray order.
fn single_nozzle_candidates(ams: Option<&AmsStatusReport>) -> Vec<Candidate> {
    let mut out = Vec::new();
    if let Some(report) = ams {
        bus_candidates(report, |_| true, &mut out);
    }
    out.sort_by_key(|c| c.order);
    out
}

/// The candidates one extruder of a dual-nozzle printer can reach, in tray order.
fn extruder_candidates(
    ams: Option<&AmsStatusReport>,
    external_spools: &[VirtualTray],
    extruder: u8,
) -> Vec<Candidate> {
    let reaches = |unit: &AmsUnit| unit_extruders(unit).contains(&extruder);
    let mut out = Vec::new();
    let mut has_ams = false;
    if let Some(report) = ams {
        has_ams = report.ams.iter().any(reaches);
        bus_candidates(report, reaches, &mut out);
    }
    // BambuStudio offers an extruder's external spool only when no AMS is bound to it.
    if !has_ams {
        let wanted_id = if extruder == EXTRUDER_DEPUTY {
            AMS_EXTERNAL_SPOOL_DEPUTY_ID
        } else {
            AMS_EXTERNAL_SPOOL_MAIN_ID
        };
        for tray in external_spools {
            if tray.id.trim().parse::<u8>() != Ok(wanted_id) {
                continue;
            }
            let Some(material) = tray.material() else {
                continue;
            };
            out.push(candidate(
                u16::from(wanted_id),
                if extruder == EXTRUDER_DEPUTY {
                    MaterialSource::ExternalSpoolLeft
                } else {
                    MaterialSource::ExternalSpoolRight
                },
                tray,
                material,
            ));
        }
    }
    out.sort_by_key(|c| c.order);
    out
}

/// The extruders a unit feeds, as BambuStudio's `GetBindedExtruderSet` reads its `info` bits 8 to 11.
///
/// No `info`: the main extruder, BambuStudio's default. `0xE`: both, when a Filament Track
/// Switch inlet is bound, else neither. Any other value is that extruder alone, so a value
/// beyond the two extruders feeds none.
fn unit_extruders(unit: &AmsUnit) -> Vec<u8> {
    if unit.has_unfixed_extruder() {
        return if unit.filament_switch_inlet().is_some() {
            vec![EXTRUDER_MAIN, EXTRUDER_DEPUTY]
        } else {
            Vec::new()
        };
    }
    vec![unit.extruder_assignment().unwrap_or(EXTRUDER_MAIN)]
}

/// Appends a candidate for every loaded tray of every AMS unit `accept` takes.
fn bus_candidates(
    report: &AmsStatusReport,
    accept: impl Fn(&AmsUnit) -> bool,
    out: &mut Vec<Candidate>,
) {
    for unit in &report.ams {
        let Some(ams_id) = unit.ams_id() else {
            continue;
        };
        if !is_bus_unit_id(ams_id) || !accept(unit) {
            continue;
        }
        for tray in unit.tray.as_deref().unwrap_or(&[]) {
            let Some(slot) = tray.slot() else {
                continue;
            };
            if !is_unit_slot(ams_id, slot) || !tray.is_loaded(ams_id) {
                continue;
            }
            let Some(material) = tray.material() else {
                continue;
            };
            // The three flat-index formulas of `.claude/rules/ams-lite-on-a2l-unit-id.md`: the
            // AMS-HT is its own id, as `ams_filament_mapping` computes it.
            let (order, source) = if is_standard_id(ams_id) {
                (
                    u16::from(ams_id) * u16::from(AMS_SLOTS_PER_UNIT) + u16::from(slot),
                    MaterialSource::StandardAms {
                        ams_id,
                        slot_id: slot,
                    },
                )
            } else if is_ams_ht_id(ams_id) {
                (u16::from(ams_id), MaterialSource::AmsHt { ams_id })
            } else if is_ams_lite_on_a2l_id(ams_id) {
                (
                    AMS_LITE_ORDER_BASE + u16::from(slot),
                    MaterialSource::AmsLite { slot_id: slot },
                )
            } else {
                continue;
            };
            out.push(candidate(order, source, tray, material));
        }
    }
}

/// Builds a candidate from a tray's cached fields, as BambuStudio's `_parse_tray_info` does.
fn candidate(order: u16, source: MaterialSource, tray: &AmsTray, material: &str) -> Candidate {
    let info_idx = tray.tray_info_idx.as_deref().unwrap_or("");
    let setting_id = tray
        .filament_setting_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(info_idx);
    Candidate {
        order,
        source,
        filament_type: tray_filament_type(material, info_idx),
        color: tray.tray_color.as_deref().map_or(WHITE, parse_color),
        filament_id: info_idx.to_string(),
        setting_id: setting_id.to_string(),
        remain: tray.remain.unwrap_or(-1),
    }
}

/// A tray's material as BambuStudio's `DevAmsTray::get_filament_type` names it.
///
/// The support-material spellings fold into `PLA-S`, `PA-S` and `ABS-S`; a bare `Support` takes
/// its kind from the tray's `tray_info_idx` (`GFS01` is PA support, anything else PLA).
fn tray_filament_type(tray_type: &str, tray_info_idx: &str) -> String {
    match tray_type {
        "Sup.PLA" | "Support W" => "PLA-S",
        "Sup.PA" | "Support G" => "PA-S",
        "Sup.ABS" => "ABS-S",
        "Support" if tray_info_idx == "GFS01" => "PA-S",
        "Support" => "PLA-S",
        other => other,
    }
    .to_string()
}

/// Parses `#RRGGBB`, `RRGGBB`, `#RRGGBBAA` or `RRGGBBAA` into `[r, g, b, a]`; anything else is opaque white.
fn parse_color(text: &str) -> [u8; 4] {
    let hex = text.trim().trim_start_matches('#');
    if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return WHITE;
    }
    let Ok(value) = u32::from_str_radix(hex, 16) else {
        return WHITE;
    };
    if hex.len() == 6 {
        let [_, r, g, b] = value.to_be_bytes();
        [r, g, b, 255]
    } else {
        value.to_be_bytes()
    }
}

/// BambuStudio's `GUI::calc_color_distance`: CIELAB ΔE76 combined with the alpha difference, clamped to `0..=1000`.
///
/// Each colour is `[r, g, b, a]`. The distance is
/// `sqrt(ΔE76(Lab1, Lab2)^2 + ((a1 - a2) * 100)^2)` with alpha in `0.0..=1.0`, and `Lab` comes
/// from `RGB2Lab` (`RGB2XYZ` with the D65 matrix of `ColorSpaceConvert.cpp`, then `XYZ2Lab`
/// against its `95.047 / 100 / 108.883` white).
#[must_use]
pub fn color_distance(a: [u8; 4], b: [u8; 4]) -> f32 {
    let lab_a = rgb_to_lab(a);
    let lab_b = rgb_to_lab(b);
    let delta_e = libm::sqrt(
        lab_a
            .iter()
            .zip(&lab_b)
            .map(|(x, y)| {
                let d = f64::from(*x) - f64::from(*y);
                d * d
            })
            .sum::<f64>(),
    ) as f32;
    let delta_alpha = (f32::from(a[3]) / CHANNEL_MAX - f32::from(b[3]) / CHANNEL_MAX) * ALPHA_SCALE;
    libm::sqrtf(delta_e * delta_e + delta_alpha * delta_alpha)
        .clamp(COLOR_DISTANCE_MIN, COLOR_DISTANCE_MAX)
}

/// BambuStudio's `PivotRGB`: sRGB gamma expansion of `n` in `0..=1`, scaled to `0..=100`.
fn pivot_rgb(n: f64) -> f64 {
    (if n > 0.04045 {
        libm::pow((n + 0.055) / 1.055, 2.4)
    } else {
        n / 12.92
    }) * 100.0
}

/// BambuStudio's `PivotXYZ`: the cube root, linear below 0.008856.
fn pivot_xyz(n: f64) -> f64 {
    if n > 0.008856 {
        libm::cbrt(n)
    } else {
        7.787 * n + 16.0 / 116.0
    }
}

/// BambuStudio's `RGB2Lab`, with the single-precision intermediates it stores between stages.
fn rgb_to_lab(color: [u8; 4]) -> [f32; 3] {
    let channel = |c: u8| pivot_rgb(f64::from(f32::from(c) / CHANNEL_MAX)) as f32;
    let (r, g, b) = (channel(color[0]), channel(color[1]), channel(color[2]));
    let x = 0.412_453_f32 * r + 0.357_580_f32 * g + 0.180_423_f32 * b;
    let y = 0.212_671_f32 * r + 0.715_160_f32 * g + 0.072_169_f32 * b;
    let z = 0.019_334_f32 * r + 0.119_193_f32 * g + 0.950_227_f32 * b;
    let fx = pivot_xyz(f64::from(x) / 95.047);
    let fy = pivot_xyz(f64::from(y) / 100.0);
    let fz = pivot_xyz(f64::from(z) / 108.883);
    [
        (116.0 * fy - 16.0) as f32,
        (500.0 * (fx - fy)) as f32,
        (200.0 * (fy - fz)) as f32,
    ]
}

/// Whether a filament and a tray name the same non-empty class id.
fn same_class(filament_id: &str, tray_id: &str) -> bool {
    !filament_id.is_empty() && filament_id == tray_id
}

/// BambuStudio's distance for one pair, `None` for a material mismatch (its hard fail).
fn pair_distance(wanted: &Wanted<'_>, tray: &Candidate) -> Option<f32> {
    if wanted.filament_type != tray.filament_type {
        return None;
    }
    let mut distance = color_distance(wanted.color, tray.color);
    let sub_class = !wanted.setting_id.is_empty() && wanted.setting_id == tray.setting_id;
    if !sub_class {
        distance += if same_class(wanted.filament_id, &tray.filament_id) {
            CLASS_DISTANCE_OFFSET
        } else {
            TYPE_DISTANCE_OFFSET
        };
    }
    Some(distance)
}

/// BambuStudio's `_prefer_tray_by_remain`: whether `candidate` should replace `picked` on an exact tie.
///
/// Only for an exact class match, and only for a tray that reports filament left: an unknown or
/// empty `picked` yields to any `candidate` that reports some, and `candidate` never wins by
/// reporting none.
fn prefer_by_remain(wanted: &Wanted<'_>, candidate: &Candidate, picked: &Candidate) -> bool {
    if !same_class(wanted.filament_id, &candidate.filament_id) || candidate.remain <= 0 {
        return false;
    }
    picked.remain <= 0 || candidate.remain < picked.remain
}

/// Matches `wanted` against `trays` greedily, returning each filament's tray index and distance.
///
/// One round per filament, as BambuStudio's loop runs. See the module doc for the rules and
/// where the indexing differs from BambuStudio's.
fn match_group(wanted: &[&Wanted<'_>], trays: &[Candidate]) -> Vec<Option<(usize, f32)>> {
    let distances: Vec<Vec<Option<f32>>> = wanted
        .iter()
        .map(|w| trays.iter().map(|t| pair_distance(w, t)).collect())
        .collect();
    let mut result: Vec<Option<(usize, f32)>> = vec![None; wanted.len()];
    let mut tray_used = vec![false; trays.len()];

    for _ in 0..wanted.len() {
        let pick = best_pair(wanted, trays, &distances, &result, &tray_used, false)
            .or_else(|| best_pair(wanted, trays, &distances, &result, &tray_used, true));
        let Some((i, j, distance)) = pick else {
            break;
        };
        result[i] = Some((j, distance));
        tray_used[j] = true;
    }
    result
}

/// One round's pick among the filaments not yet mapped.
///
/// Without `allow_used`, a used tray is a candidate only below [`REUSE_DISTANCE`]; with it, any
/// tray of the right material is.
fn best_pair(
    wanted: &[&Wanted<'_>],
    trays: &[Candidate],
    distances: &[Vec<Option<f32>>],
    result: &[Option<(usize, f32)>],
    tray_used: &[bool],
    allow_used: bool,
) -> Option<(usize, usize, f32)> {
    let mut best: Option<(usize, usize, f32)> = None;
    for (i, row) in distances.iter().enumerate() {
        if result[i].is_some() {
            continue;
        }
        for (j, distance) in row.iter().enumerate() {
            let Some(distance) = *distance else {
                continue;
            };
            if tray_used[j] && !allow_used && distance >= REUSE_DISTANCE {
                continue;
            }
            let better = match best {
                None => true,
                Some((bi, bj, best_distance)) => {
                    distance < best_distance
                        || (distance == best_distance
                            && ((!same_class(wanted[bi].filament_id, &trays[bj].filament_id)
                                && same_class(wanted[i].filament_id, &trays[j].filament_id))
                                || prefer_by_remain(wanted[i], &trays[j], &trays[bj])))
                }
            };
            if better {
                best = Some((i, j, distance));
            }
        }
    }
    best
}

#[cfg(test)]
mod tests;
