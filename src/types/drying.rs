//! # Filament Drying Presets
//!
//! Static per-material drying parameters, as the printer's own drying screen uses them.
//!
//! [`start_drying`](crate::PrinterClient::start_drying) takes a bare `filament: &str` and a
//! `temp`, leaving a consumer to source both. This module is the vendor's own answer: one entry
//! per naked material type, each carrying the temperature, duration and cooling temperature
//! BambuStudio fills in when that material is picked.
//!
//! **This is not the filament-*preset* list.** `tray_info_idx`/`setting_id` name brands and
//! variants (`"Bambu PLA Basic"`); these are the base material types behind them, one per
//! `fdm_filament_<type>.json` profile in BambuStudio's own resource tree.
//!
//! All values transcribed from `resources/profiles/BBL/filament/fdm_filament_*.json` at
//! BambuStudio, read directly rather than from a secondary account of them. bambuddy
//! corroborates the idle column independently (`DEFAULT_DRYING_PRESETS`,
//! `print_scheduler.py:777-786`) and selects the per-unit column the same way (line 3807,
//! `temp_key = module_type if module_type in ("n3f", "n3s") else "n3f"`).

use crate::types::telemetry::AmsUnitModel;

/// A base filament material with vendor-published drying parameters.
///
/// Each material's profile stores temperature and time as a 4-element array indexed
/// `[N3F idle, N3S idle, N3F printing, N3S printing]` — the mapping is explicit in
/// BambuStudio's `DevUtilBackend.cpp:87-91`, which reads exactly those four positions into
/// `..._on_idle[N3F]`, `..._on_idle[N3S]`, `..._on_print[N3F]`, `..._on_print[N3S]`. Everything
/// here is indexed the same way, via [`AmsUnitModel`] plus a `printing` flag.
///
/// **A convenience layer, not a replacement for the `&str` parameter.** The wire `dry_filament`
/// field is free-form — BambuStudio sends the tray's own `filament_type` string — so
/// [`start_drying`](crate::PrinterClient::start_drying) keeps taking an arbitrary `&str` and
/// this enum stays open at the edges via [`from_filament_type`](Self::from_filament_type)
/// returning `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DryingMaterial {
    /// Polylactic acid.
    Pla,
    /// Polyethylene terephthalate glycol. Profile `fdm_filament_pet.json`.
    Petg,
    /// Copolyester (PCTG).
    Pctg,
    /// Acrylonitrile butadiene styrene.
    Abs,
    /// Acrylonitrile styrene acrylate.
    Asa,
    /// High-impact polystyrene.
    Hips,
    /// Polycarbonate.
    Pc,
    /// Polyamide (nylon).
    Pa,
    /// Polyvinyl alcohol (support material).
    Pva,
    /// Butenediol vinyl alcohol copolymer (support material).
    Bvoh,
    /// Thermoplastic polyurethane.
    Tpu,
    /// Polypropylene.
    Pp,
    /// Polyethylene.
    Pe,
    /// Polyhydroxyalkanoate.
    Pha,
    /// Ethylene-vinyl acetate.
    Eva,
    /// Polyphthalamide. Profile `fdm_filament_ppa.json`; sold as PPA-CF.
    Ppa,
    /// Polyphenylene sulfide.
    Pps,
}

/// Every material this table carries, in profile order.
///
/// Seventeen entries — one per non-template, non-common `fdm_filament_*.json` profile.
const ALL: [DryingMaterial; 17] = [
    DryingMaterial::Pla,
    DryingMaterial::Petg,
    DryingMaterial::Pctg,
    DryingMaterial::Abs,
    DryingMaterial::Asa,
    DryingMaterial::Hips,
    DryingMaterial::Pc,
    DryingMaterial::Pa,
    DryingMaterial::Pva,
    DryingMaterial::Bvoh,
    DryingMaterial::Tpu,
    DryingMaterial::Pp,
    DryingMaterial::Pe,
    DryingMaterial::Pha,
    DryingMaterial::Eva,
    DryingMaterial::Ppa,
    DryingMaterial::Pps,
];

/// Fallback `cooling_temp` BambuStudio sends when a tray's filament has no drying preset
/// (`AMSDryControl.cpp:813`, `int cooling_temp = 50;`).
pub const DEFAULT_COMMAND_COOLING_TEMP: i32 = 50;

impl DryingMaterial {
    /// Every material in this table.
    #[must_use]
    pub fn all() -> &'static [DryingMaterial] {
        &ALL
    }

    /// Matches a wire `filament_type` string to a material, case-insensitively.
    ///
    /// Accepts the bare material name and the common composite suffixes that share a base
    /// profile — `"PA-CF"`, `"PAHT-CF"` and `"PA6-GF"` all resolve to [`Pa`](Self::Pa), because
    /// BambuStudio's own composite presets inherit their drying parameters from the base
    /// `fdm_filament_pa.json`. `None` for anything unrecognized, which is the case the free-form
    /// `&str` parameter on `start_drying` exists to serve.
    #[must_use]
    pub fn from_filament_type(filament_type: &str) -> Option<Self> {
        let trimmed = filament_type.trim();
        // Composite grades are spelled `<base>-CF`, `<base>-GF`, `<base>-CF10` and so on. The
        // base material before the first `-` is what carries the drying profile.
        let base = trimmed.split('-').next().unwrap_or(trimmed);
        let matches = |name: &str| base.eq_ignore_ascii_case(name);

        // `PAHT`/`PA6`/`PA12` are nylons; check the prefix rather than enumerating grades.
        if base.len() >= 2 && base[..2].eq_ignore_ascii_case("PA") && !matches("PPA") {
            return Some(Self::Pa);
        }
        ALL.iter()
            .copied()
            .find(|m| matches(m.wire_name()))
            // `PET` appears on the wire for what the profile calls PETG.
            .or_else(|| matches("PET").then_some(Self::Petg))
    }

    /// The canonical material name, as it appears in a wire `filament_type` field.
    #[must_use]
    pub fn wire_name(self) -> &'static str {
        match self {
            Self::Pla => "PLA",
            Self::Petg => "PETG",
            Self::Pctg => "PCTG",
            Self::Abs => "ABS",
            Self::Asa => "ASA",
            Self::Hips => "HIPS",
            Self::Pc => "PC",
            Self::Pa => "PA",
            Self::Pva => "PVA",
            Self::Bvoh => "BVOH",
            Self::Tpu => "TPU",
            Self::Pp => "PP",
            Self::Pe => "PE",
            Self::Pha => "PHA",
            Self::Eva => "EVA",
            Self::Ppa => "PPA",
            Self::Pps => "PPS",
        }
    }

    /// `[N3F idle, N3S idle, N3F printing, N3S printing]` drying temperatures in °C.
    const fn temp_row(self) -> [u32; 4] {
        match self {
            Self::Pla => [45, 45, 45, 45],
            Self::Petg => [65, 65, 55, 55],
            Self::Pctg => [65, 65, 55, 55],
            Self::Abs => [65, 80, 65, 75],
            Self::Asa => [65, 80, 65, 80],
            Self::Hips => [65, 80, 65, 75],
            Self::Pc => [65, 80, 65, 80],
            Self::Pa => [65, 85, 65, 85],
            Self::Pva => [65, 85, 65, 70],
            Self::Bvoh => [60, 60, 45, 45],
            Self::Tpu => [65, 75, 45, 45],
            Self::Pp => [60, 60, 50, 50],
            Self::Pe => [45, 45, 45, 45],
            Self::Pha => [45, 45, 45, 45],
            Self::Eva => [45, 45, 45, 45],
            Self::Ppa => [65, 85, 65, 85],
            Self::Pps => [65, 85, 65, 85],
        }
    }

    /// `[N3F idle, N3S idle, N3F printing, N3S printing]` drying durations in hours.
    ///
    /// The profiles store these as decimal strings (`"8.0"`); every published value is integral,
    /// so they are carried as whole hours here — matching the wire `duration` field, which is
    /// also whole hours.
    const fn hours_row(self) -> [u32; 4] {
        match self {
            Self::Abs | Self::Pc => [12, 8, 12, 8],
            Self::Pva | Self::Tpu => [12, 18, 12, 18],
            _ => [12, 12, 12, 12],
        }
    }

    /// Index into a 4-element profile row, or `None` if this unit has no drying chamber.
    const fn row_index(unit: AmsUnitModel, printing: bool) -> Option<usize> {
        let base = match unit {
            AmsUnitModel::Ams2Pro => 0,
            AmsUnitModel::AmsHt => 1,
            _ => return None,
        };
        Some(if printing { base + 2 } else { base })
    }

    /// Vendor default drying temperature in °C for this material on this unit.
    ///
    /// `printing` selects the lower while-printing column, which exists because the AMS sits in
    /// the print's thermal envelope. `None` for a unit with no drying chamber.
    ///
    /// **This is a starting value, not a bound.** It always falls inside
    /// [`AmsUnitModel::dry_temp_range`], but the range is the hardware limit and this is the
    /// vendor's recommendation within it. BambuStudio additionally floors the printing-column
    /// value at use: `min(printing_temp, softening_temp, heat_distortion_temp)`
    /// (`AMSDryControl.cpp:1723-1725`) — see [`softening_temp`](Self::softening_temp) and
    /// [`heat_distortion_temp`](Self::heat_distortion_temp) to reproduce that clamp.
    #[must_use]
    pub fn default_temp(self, unit: AmsUnitModel, printing: bool) -> Option<u32> {
        Self::row_index(unit, printing).map(|i| self.temp_row()[i])
    }

    /// Vendor default drying duration in whole hours for this material on this unit.
    ///
    /// `None` for a unit with no drying chamber.
    #[must_use]
    pub fn default_duration_hours(self, unit: AmsUnitModel, printing: bool) -> Option<u32> {
        Self::row_index(unit, printing).map(|i| self.hours_row()[i])
    }

    /// Temperature (°C) at which this material begins to soften
    /// (`filament_dev_drying_softening_temperature`).
    ///
    /// Also the value to pass as `start_drying`'s `cooling_temp` — see
    /// [`command_cooling_temp`](Self::command_cooling_temp).
    #[must_use]
    pub fn softening_temp(self) -> u32 {
        match self {
            Self::Pla | Self::Tpu | Self::Bvoh | Self::Eva | Self::Pe | Self::Pha => 50,
            Self::Petg | Self::Pctg => 60,
            Self::Pp => 55,
            Self::Abs | Self::Hips => 80,
            Self::Asa => 85,
            Self::Pc => 90,
            Self::Pva => 75,
            Self::Pa | Self::Ppa | Self::Pps => 150,
        }
    }

    /// What to send as `start_drying`'s `cooling_temp` for this material.
    ///
    /// **The wire `cooling_temp` carries the *softening* temperature, not the profile's
    /// `filament_dev_drying_cooling_temperature`.** That second field exists and BambuStudio
    /// parses it (`DevUtilBackend.cpp:109-110`), but never sends it — the drying command is
    /// built from `filament_dev_drying_softening_temperature` (`AMSDryControl.cpp:816`). Reading
    /// the similarly-named field instead is the obvious mistake here, so this accessor exists to
    /// make the right one the easy one.
    ///
    /// Equal to [`softening_temp`](Self::softening_temp); see
    /// [`DEFAULT_COMMAND_COOLING_TEMP`] for what BambuStudio sends when a tray's filament
    /// resolves to no preset at all.
    #[must_use]
    pub fn command_cooling_temp(self) -> i32 {
        self.softening_temp() as i32
    }

    /// Heat-distortion temperature (°C)
    /// (`filament_dev_ams_drying_heat_distortion_temperature`), where the profile publishes one.
    ///
    /// `None` for [`Pe`](Self::Pe) and [`Pha`](Self::Pha), whose profiles omit the key. One of
    /// the three inputs to BambuStudio's while-printing clamp
    /// (`min(printing_temp, softening_temp, heat_distortion_temp)`, `AMSDryControl.cpp:1723-1725`).
    #[must_use]
    pub fn heat_distortion_temp(self) -> Option<u32> {
        Some(match self {
            Self::Pla | Self::Tpu | Self::Eva => 45,
            Self::Pp => 60,
            Self::Bvoh => 65,
            Self::Petg | Self::Pctg | Self::Pva => 75,
            Self::Abs | Self::Hips => 90,
            Self::Asa => 100,
            Self::Pc => 105,
            Self::Pa | Self::Ppa | Self::Pps => 165,
            Self::Pe | Self::Pha => return None,
        })
    }

    /// Returns true if this unit can dry this material *completely*.
    ///
    /// A `false` here does not mean "don't dry it" — BambuStudio still permits the cycle and
    /// shows "This filament may not be completely dried" (`AMSDryControl.cpp:1203`). It means
    /// the cycle will not fully remove the moisture.
    ///
    /// Read from `filament_dev_ams_drying_ams_limitations`, whose values do **not** use the
    /// `DevAmsType` numbering the rest of this module does: in that field `"0"` is the AMS 2 Pro
    /// and `"1"` the AMS-HT (`s_ams_type_map`, `DevUtilBackend.cpp:58-61`), where `DevAmsType`
    /// makes them `3` and `4`. `["-1"]` means neither unit qualifies.
    ///
    /// **A profile that omits the key entirely behaves exactly like `["-1"]`.** BambuStudio
    /// builds an empty set when the key is absent and then warns on set non-membership
    /// (`AMSDryControl.cpp:1184` and `1203`), so the seven materials with no key published —
    /// ABS, ASA, HIPS, PC, PA, PVA, TPU — get the same warning as PPA and PPS, which name
    /// `["-1"]` explicitly. This method reports `false` for all nine rather than treating an
    /// absent key as permission.
    #[must_use]
    pub fn fully_dryable_by(self, unit: AmsUnitModel) -> bool {
        if !unit.supports_drying() {
            return false;
        }
        matches!(
            self,
            Self::Pla
                | Self::Petg
                | Self::Pctg
                | Self::Bvoh
                | Self::Pp
                | Self::Pe
                | Self::Pha
                | Self::Eva
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The four-position row layout is the whole contract of this module — a transposed
    /// idle/printing pair or a swapped 2Pro/HT column would publish a plausible wrong
    /// temperature. Pinned against materials whose columns all differ.
    #[test]
    fn test_row_indexing_matches_upstream_layout() {
        // ABS: [N3F idle 65, N3S idle 80, N3F print 65, N3S print 75].
        let abs = DryingMaterial::Abs;
        assert_eq!(abs.default_temp(AmsUnitModel::Ams2Pro, false), Some(65));
        assert_eq!(abs.default_temp(AmsUnitModel::AmsHt, false), Some(80));
        assert_eq!(abs.default_temp(AmsUnitModel::Ams2Pro, true), Some(65));
        assert_eq!(abs.default_temp(AmsUnitModel::AmsHt, true), Some(75));
        // ABS hours: 12 on the 2 Pro, 8 on the HT.
        assert_eq!(
            abs.default_duration_hours(AmsUnitModel::Ams2Pro, false),
            Some(12)
        );
        assert_eq!(
            abs.default_duration_hours(AmsUnitModel::AmsHt, false),
            Some(8)
        );

        // PVA is the row where the HT printing column drops below its idle column (85 -> 70)
        // and hours go up rather than down.
        let pva = DryingMaterial::Pva;
        assert_eq!(pva.default_temp(AmsUnitModel::AmsHt, false), Some(85));
        assert_eq!(pva.default_temp(AmsUnitModel::AmsHt, true), Some(70));
        assert_eq!(
            pva.default_duration_hours(AmsUnitModel::AmsHt, false),
            Some(18)
        );
    }

    /// Every published default must sit inside the hardware range the client enforces,
    /// otherwise picking a vendor default would produce a command `start_drying` rejects.
    #[test]
    fn test_every_default_temp_is_inside_the_unit_range() {
        for material in DryingMaterial::all() {
            for unit in [AmsUnitModel::Ams2Pro, AmsUnitModel::AmsHt] {
                let (min, max) = unit.dry_temp_range().expect("drying unit has a range");
                for printing in [false, true] {
                    let temp = material
                        .default_temp(unit, printing)
                        .expect("drying unit has a default");
                    assert!(
                        temp >= min && temp <= max,
                        "{material:?} on {unit:?} (printing={printing}) is {temp}, outside {min}-{max}"
                    );
                }
            }
        }
    }

    /// A unit with no drying chamber has no column in the table at all.
    #[test]
    fn test_heaterless_units_have_no_defaults() {
        for unit in [
            AmsUnitModel::ExternalSpool,
            AmsUnitModel::Ams,
            AmsUnitModel::AmsLite,
            AmsUnitModel::AmsLiteMixed,
        ] {
            for printing in [false, true] {
                assert_eq!(DryingMaterial::Pla.default_temp(unit, printing), None);
                assert_eq!(
                    DryingMaterial::Pla.default_duration_hours(unit, printing),
                    None
                );
            }
            assert!(!DryingMaterial::Pla.fully_dryable_by(unit));
        }
    }

    #[test]
    fn test_from_filament_type_accepts_wire_spellings() {
        // Bare names, case-insensitively.
        assert_eq!(
            DryingMaterial::from_filament_type("PLA"),
            Some(DryingMaterial::Pla)
        );
        assert_eq!(
            DryingMaterial::from_filament_type("abs"),
            Some(DryingMaterial::Abs)
        );

        // Composite grades inherit the base material's profile.
        for spelling in ["PA-CF", "PAHT-CF", "PA6-GF", "PA"] {
            assert_eq!(
                DryingMaterial::from_filament_type(spelling),
                Some(DryingMaterial::Pa),
                "{spelling}"
            );
        }

        // PPA starts with "PP" and must not be swallowed by either the PA prefix rule or PP.
        assert_eq!(
            DryingMaterial::from_filament_type("PPA-CF"),
            Some(DryingMaterial::Ppa)
        );
        assert_eq!(
            DryingMaterial::from_filament_type("PP"),
            Some(DryingMaterial::Pp)
        );

        // The profile is named `pet`; the wire and the UI both say PETG.
        for spelling in ["PETG", "PET"] {
            assert_eq!(
                DryingMaterial::from_filament_type(spelling),
                Some(DryingMaterial::Petg),
                "{spelling}"
            );
        }

        // Unrecognized input stays None — that is what the free-form `&str` on start_drying is
        // for, rather than guessing a material and its temperature.
        assert_eq!(DryingMaterial::from_filament_type("Unobtainium"), None);
        assert_eq!(DryingMaterial::from_filament_type(""), None);
    }

    /// A round trip through the canonical name must land back on the same material, or
    /// `wire_name` and `from_filament_type` disagree about spelling.
    #[test]
    fn test_wire_name_round_trips() {
        for material in DryingMaterial::all() {
            assert_eq!(
                DryingMaterial::from_filament_type(material.wire_name()),
                Some(*material),
                "{material:?}"
            );
        }
    }

    /// The command's `cooling_temp` is the *softening* temperature, not the profile's
    /// similarly-named `filament_dev_drying_cooling_temperature` — BambuStudio parses that
    /// second field and never sends it.
    #[test]
    fn test_command_cooling_temp_is_the_softening_temperature() {
        // PLA: softening 50, profile cooling 45. The wire value is 50.
        assert_eq!(DryingMaterial::Pla.softening_temp(), 50);
        assert_eq!(DryingMaterial::Pla.command_cooling_temp(), 50);
        // TPU: softening 50, profile cooling 40.
        assert_eq!(DryingMaterial::Tpu.command_cooling_temp(), 50);
        // ASA: softening 85, profile cooling 85 — equal here, which is why a material where
        // they differ has to be the one pinning the behavior.
        assert_eq!(DryingMaterial::Asa.command_cooling_temp(), 85);

        for material in DryingMaterial::all() {
            assert_eq!(
                material.command_cooling_temp(),
                material.softening_temp() as i32,
                "{material:?}"
            );
        }
    }

    #[test]
    fn test_fully_dryable_follows_upstream_including_absent_key() {
        // Explicit `["1", "0"]` — both units qualify.
        for material in [
            DryingMaterial::Pla,
            DryingMaterial::Petg,
            DryingMaterial::Pctg,
            DryingMaterial::Bvoh,
            DryingMaterial::Pp,
            DryingMaterial::Pe,
            DryingMaterial::Pha,
            DryingMaterial::Eva,
        ] {
            assert!(
                material.fully_dryable_by(AmsUnitModel::Ams2Pro),
                "{material:?}"
            );
            assert!(
                material.fully_dryable_by(AmsUnitModel::AmsHt),
                "{material:?}"
            );
        }

        // Explicit `["-1"]` — neither qualifies.
        for material in [DryingMaterial::Ppa, DryingMaterial::Pps] {
            assert!(
                !material.fully_dryable_by(AmsUnitModel::Ams2Pro),
                "{material:?}"
            );
            assert!(
                !material.fully_dryable_by(AmsUnitModel::AmsHt),
                "{material:?}"
            );
        }

        // Key absent from the profile. BambuStudio builds an empty set and warns on
        // non-membership, so these behave identically to `["-1"]` rather than as permission.
        for material in [
            DryingMaterial::Abs,
            DryingMaterial::Asa,
            DryingMaterial::Hips,
            DryingMaterial::Pc,
            DryingMaterial::Pa,
            DryingMaterial::Pva,
            DryingMaterial::Tpu,
        ] {
            assert!(
                !material.fully_dryable_by(AmsUnitModel::Ams2Pro),
                "{material:?} has no ams_limitations key and must not claim full drying"
            );
        }
    }

    #[test]
    fn test_heat_distortion_temp_absent_only_where_profile_omits_it() {
        // `fdm_filament_pe.json` and `fdm_filament_pha.json` publish no HDT value.
        assert_eq!(DryingMaterial::Pe.heat_distortion_temp(), None);
        assert_eq!(DryingMaterial::Pha.heat_distortion_temp(), None);
        for material in DryingMaterial::all() {
            if matches!(material, DryingMaterial::Pe | DryingMaterial::Pha) {
                continue;
            }
            assert!(
                material.heat_distortion_temp().is_some(),
                "{material:?} publishes an HDT value"
            );
        }
    }
}
