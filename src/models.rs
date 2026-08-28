//! # Printer Model Identification
//!
//! Every Bambu Lab printer has a 3-character serial number prefix that identifies
//! its model. [`PrinterModel`] enumerates all known models, and [`resolve_model()`]
//! maps serial prefixes (with an SSDP `DevModel` fallback) to the right variant.
//! The resolved model drives behavioral dispatch through the [`crate::quirks`] engine.
//!
//! `MODELS` is the single source of truth: one row per supported model, carrying its
//! serial prefix, its wire-protocol tokens, and its human-readable name.
//! [`resolve_model()`], [`supported_models()`], and [`PrinterModel::display_name()`] are
//! all views over that table, so adding a model means adding one enum variant and one row.

use core::fmt;

/// Enumeration of physical Bambu Lab printer models supported on the local interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrinterModel {
    /// X1 and X1C Series (CoreXY architecture, RTSP-capable)
    X1C,
    /// X1E (Enterprise CoreXY architecture, wired Ethernet)
    X1E,
    /// X2D Series (CoreXY architecture, dual auxiliary cooling)
    X2D,
    /// A1 Mini (Constrained bed-slinger, binary camera stream)
    A1Mini,
    /// A1 (Standard bed-slinger, binary camera stream)
    A1,
    /// A2L Series
    A2L,
    /// P1P (Early CoreXY architecture, binary camera stream)
    P1P,
    /// P1S (Enclosed CoreXY architecture, binary camera stream)
    P1S,
    /// P2S Series (RTSP-capable)
    P2S,
    /// H2D (Dual-nozzle IDEX platform)
    H2D,
    /// H2D Pro (Premium IDEX platform)
    H2DPro,
    /// H2C (Vortek tool-changer + fixed hotend, 7 nozzles total)
    H2C,
    /// H2S (Single-nozzle platform sharing H2 mechanics)
    H2S,
    /// Fallback variant for newly released or unrecognized printer targets
    Unknown,
}

/// Everything known about one supported model, as one row of `MODELS`.
///
/// Deliberately private: a public `serial_prefix` field would permanently commit to one
/// prefix per model, and multiple wire tokens per model is already real (`O1E` and `O2D`
/// both resolve to H2D Pro). The accessors on [`PrinterModel`] return types that can
/// widen later without breaking callers.
struct ModelSpec {
    /// The variant this row describes.
    model: PrinterModel,
    /// Human-readable name for presentation. **Never matched against.**
    ///
    /// Kept distinct from `dev_tokens` on purpose: a token like `"A1 Mini"` arriving in an
    /// SSDP URN is a wire value that merely looks human-readable, not a display name.
    /// Conflating the two would make the user-facing name hostage to firmware spelling.
    /// Values here follow Bambu's own naming, matching `MODEL_MATRIX.csv`.
    display_name: &'static str,
    /// Physical serial number prefix, confirmed by the Bambu Lab wiki.
    serial_prefix: &'static str,
    /// SSDP `DevModel` header values: short model *codes* (`BL-P001`, `C12`, ...) plus,
    /// per [REF-NET-DISC] Protocol Violation #7, the display-name segment (`P1S`, `X1C`,
    /// ...) some firmware tracks embed in the NT/ST URN instead —
    /// `extract_model_from_nt_st` hands that raw URN segment off through the same
    /// `dev_model` parameter, so both sets of values belong here.
    ///
    /// Matching is case-insensitive, so a spelling differing from another token only by
    /// case does not need its own entry.
    dev_tokens: &'static [&'static str],
}

/// The supported-model table — one row per [`PrinterModel`] variant except
/// [`PrinterModel::Unknown`], which is the unrecognized-target fallback, not a model.
const MODELS: &[ModelSpec] = &[
    ModelSpec {
        model: PrinterModel::X1C,
        display_name: "X1C",
        serial_prefix: "00M",
        dev_tokens: &["BL-P001", "X1", "X1C"],
    },
    ModelSpec {
        model: PrinterModel::X1E,
        display_name: "X1E",
        serial_prefix: "03W",
        dev_tokens: &["C13", "X1E"],
    },
    ModelSpec {
        model: PrinterModel::X2D,
        display_name: "X2D",
        serial_prefix: "20P",
        dev_tokens: &["N6", "X2D"],
    },
    ModelSpec {
        model: PrinterModel::A1Mini,
        display_name: "A1 mini",
        serial_prefix: "030",
        dev_tokens: &["N1", "A1 Mini", "A1Mini"],
    },
    ModelSpec {
        model: PrinterModel::A1,
        display_name: "A1",
        serial_prefix: "039",
        dev_tokens: &["N2S", "A1"],
    },
    ModelSpec {
        model: PrinterModel::A2L,
        display_name: "A2L",
        serial_prefix: "26A",
        dev_tokens: &["N9", "A2L"],
    },
    ModelSpec {
        model: PrinterModel::P1P,
        display_name: "P1P",
        serial_prefix: "01S",
        dev_tokens: &["C11", "P1P"],
    },
    ModelSpec {
        model: PrinterModel::P1S,
        display_name: "P1S",
        serial_prefix: "01P",
        dev_tokens: &["C12", "P1S"],
    },
    ModelSpec {
        model: PrinterModel::P2S,
        display_name: "P2S",
        serial_prefix: "22E",
        dev_tokens: &["N7", "P2S"],
    },
    ModelSpec {
        model: PrinterModel::H2D,
        display_name: "H2D",
        serial_prefix: "094",
        dev_tokens: &["O1D", "H2D"],
    },
    ModelSpec {
        model: PrinterModel::H2DPro,
        display_name: "H2D Pro",
        serial_prefix: "239",
        dev_tokens: &["O1E", "O2D", "H2D Pro", "H2DPro"],
    },
    ModelSpec {
        model: PrinterModel::H2C,
        display_name: "H2C",
        serial_prefix: "31B",
        dev_tokens: &["O1C", "O1C2", "H2C"],
    },
    ModelSpec {
        model: PrinterModel::H2S,
        display_name: "H2S",
        serial_prefix: "093",
        dev_tokens: &["O1S", "H2S"],
    },
];

/// Display name for [`PrinterModel::Unknown`], which has no `MODELS` row.
const UNKNOWN_DISPLAY_NAME: &str = "Unknown";

/// Returns every printer model this crate supports, in table order.
///
/// [`PrinterModel::Unknown`] is excluded: it is the fallback for targets this crate does
/// not recognize, not a supported model. Pair each item with [`PrinterModel::quirks`] to
/// build a capability matrix without matching on variants.
///
/// [`PrinterModel::quirks`]: PrinterModel::quirks
pub fn supported_models() -> impl Iterator<Item = PrinterModel> {
    MODELS.iter().map(|spec| spec.model)
}

impl PrinterModel {
    /// Returns this model's `MODELS` row, or `None` for [`PrinterModel::Unknown`].
    fn spec(self) -> Option<&'static ModelSpec> {
        MODELS.iter().find(|spec| spec.model == self)
    }

    /// Returns the human-readable model name, e.g. `"H2D Pro"` or `"A1 mini"`.
    ///
    /// Follows Bambu's own naming, so it is safe to show to a user directly.
    /// [`PrinterModel::Unknown`] renders as `"Unknown"`.
    pub fn display_name(self) -> &'static str {
        match self.spec() {
            Some(spec) => spec.display_name,
            None => UNKNOWN_DISPLAY_NAME,
        }
    }

    /// Returns the 3-character serial number prefix identifying this model.
    ///
    /// `None` for [`PrinterModel::Unknown`]. Useful for validating a serial before
    /// attempting a connection.
    pub fn serial_prefix(self) -> Option<&'static str> {
        self.spec().map(|spec| spec.serial_prefix)
    }
}

impl fmt::Display for PrinterModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

/// Resolves the specific printer model using physical serial number prefixes combined with target SSDP model advertisements as a secondary signal.
///
/// Each H2-series model has a distinct serial prefix confirmed by the Bambu Lab wiki:
/// `094` = H2D, `093` = H2S, `239` = H2D Pro, `31B` = H2C. When the prefix is
/// unrecognized, the optional `DevModel` SSDP header provides a fallback path.
///
/// The two lookups are **separate full passes over the table**, and must stay that way:
/// a serial prefix on any row outranks a `dev_model` token on every row. Folding them
/// into a single pass would let an earlier row's token beat a later row's prefix,
/// silently changing which signal wins when the two disagree.
///
/// Both `serial` and `dev_model` are matched case-insensitively: SSDP USN serial casing
/// varies by firmware compile target (reference/01_network_discovery.md §1.6), and a
/// caller can also pass either value straight into [`PrinterIdentity::new`] with no
/// discovery-layer normalization.
///
/// [`PrinterIdentity::new`]: crate::identity::PrinterIdentity::new
pub fn resolve_model(serial: &str, dev_model: Option<&str>) -> PrinterModel {
    let prefix = serial.get(0..3).unwrap_or("");

    if let Some(spec) = MODELS
        .iter()
        .find(|spec| prefix.eq_ignore_ascii_case(spec.serial_prefix))
    {
        return spec.model;
    }

    if let Some(m) = dev_model
        && let Some(spec) = MODELS
            .iter()
            .find(|spec| spec.dev_tokens.iter().any(|t| m.eq_ignore_ascii_case(t)))
    {
        return spec.model;
    }

    PrinterModel::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_h2_distinct_prefix_resolution() {
        assert_eq!(resolve_model("09406A521703533", None), PrinterModel::H2D);
        assert_eq!(resolve_model("09306A521703533", None), PrinterModel::H2S);
        assert_eq!(resolve_model("23906A521703533", None), PrinterModel::H2DPro);
        assert_eq!(resolve_model("31B06A521703533", None), PrinterModel::H2C);
    }

    #[test]
    fn test_h2_dev_model_fallback() {
        assert_eq!(resolve_model("999000000", Some("O1D")), PrinterModel::H2D);
        assert_eq!(resolve_model("999000000", Some("O1S")), PrinterModel::H2S);
        assert_eq!(
            resolve_model("999000000", Some("O1E")),
            PrinterModel::H2DPro
        );
        assert_eq!(
            resolve_model("999000000", Some("O2D")),
            PrinterModel::H2DPro
        );
        assert_eq!(resolve_model("999000000", Some("O1C")), PrinterModel::H2C);
        assert_eq!(resolve_model("999000000", Some("O1C2")), PrinterModel::H2C);
    }

    #[test]
    fn test_all_prefix_resolution() {
        assert_eq!(resolve_model("00M123456789", None), PrinterModel::X1C);
        assert_eq!(resolve_model("03W123456789", None), PrinterModel::X1E);
        assert_eq!(resolve_model("20P123456789", None), PrinterModel::X2D);
        assert_eq!(resolve_model("01S123456789", None), PrinterModel::P1P);
        assert_eq!(resolve_model("01P123456789", None), PrinterModel::P1S);
        assert_eq!(resolve_model("22E123456789", None), PrinterModel::P2S);
        assert_eq!(resolve_model("030123456789", None), PrinterModel::A1Mini);
        assert_eq!(resolve_model("039123456789", None), PrinterModel::A1);
        assert_eq!(resolve_model("26A123456789", None), PrinterModel::A2L);
        assert_eq!(resolve_model("094123456789", None), PrinterModel::H2D);
        assert_eq!(resolve_model("093123456789", None), PrinterModel::H2S);
        assert_eq!(resolve_model("239123456789", None), PrinterModel::H2DPro);
        assert_eq!(resolve_model("31B123456789", None), PrinterModel::H2C);
    }

    #[test]
    fn test_dev_model_fallback() {
        assert_eq!(
            resolve_model("999000000", Some("BL-P001")),
            PrinterModel::X1C
        );
        assert_eq!(resolve_model("999000000", Some("C13")), PrinterModel::X1E);
        assert_eq!(resolve_model("999000000", Some("N6")), PrinterModel::X2D);
        assert_eq!(resolve_model("999000000", Some("N1")), PrinterModel::A1Mini);
        assert_eq!(resolve_model("999000000", Some("N2S")), PrinterModel::A1);
        assert_eq!(resolve_model("999000000", Some("N9")), PrinterModel::A2L);
        assert_eq!(resolve_model("999000000", Some("C11")), PrinterModel::P1P);
        assert_eq!(resolve_model("999000000", Some("C12")), PrinterModel::P1S);
        assert_eq!(resolve_model("999000000", Some("N7")), PrinterModel::P2S);
        assert_eq!(
            resolve_model("999000000", Some("FUTURE")),
            PrinterModel::Unknown
        );
        assert_eq!(resolve_model("999000000", None), PrinterModel::Unknown);
    }

    #[test]
    fn test_short_and_empty_serial() {
        assert_eq!(resolve_model("", None), PrinterModel::Unknown);
        assert_eq!(resolve_model("AB", None), PrinterModel::Unknown);
        assert_eq!(resolve_model("00", None), PrinterModel::Unknown);
        assert_eq!(resolve_model("", Some("C12")), PrinterModel::P1S);
    }

    #[test]
    fn test_nt_st_display_name_fallback() {
        // BUG #36: NT/ST-embedded display names (e.g. "P1S" from
        // urn:bambulab-com:device:P1S:1) must resolve, not just DevModel codes.
        assert_eq!(resolve_model("999000000", Some("P1S")), PrinterModel::P1S);
        assert_eq!(resolve_model("999000000", Some("X1C")), PrinterModel::X1C);
        assert_eq!(resolve_model("999000000", Some("X1E")), PrinterModel::X1E);
        assert_eq!(resolve_model("999000000", Some("X2D")), PrinterModel::X2D);
        assert_eq!(resolve_model("999000000", Some("P1P")), PrinterModel::P1P);
        assert_eq!(resolve_model("999000000", Some("P2S")), PrinterModel::P2S);
        assert_eq!(resolve_model("999000000", Some("A1")), PrinterModel::A1);
        assert_eq!(
            resolve_model("999000000", Some("A1 Mini")),
            PrinterModel::A1Mini
        );
        assert_eq!(resolve_model("999000000", Some("A2L")), PrinterModel::A2L);
        assert_eq!(resolve_model("999000000", Some("H2C")), PrinterModel::H2C);
        assert_eq!(resolve_model("999000000", Some("H2D")), PrinterModel::H2D);
        assert_eq!(
            resolve_model("999000000", Some("H2D Pro")),
            PrinterModel::H2DPro
        );
        assert_eq!(resolve_model("999000000", Some("H2S")), PrinterModel::H2S);
    }

    #[test]
    fn test_case_insensitive_resolution() {
        // BUG #53: serial-prefix and dev_model matching must not depend on casing --
        // firmware compile targets vary USN/DevModel casing, and PrinterIdentity::new
        // has no discovery-layer normalization of its own.
        assert_eq!(resolve_model("01p123456789", None), PrinterModel::P1S);
        assert_eq!(resolve_model("999000000", Some("p1s")), PrinterModel::P1S);
        assert_eq!(resolve_model("999000000", Some("c12")), PrinterModel::P1S);
        assert_eq!(resolve_model("999000000", Some("o1d")), PrinterModel::H2D);
    }

    #[test]
    fn test_serial_prefix_outranks_conflicting_dev_model() {
        // The two lookups must stay separate full passes over the table. A single pass
        // testing serial_prefix and dev_tokens together would let an earlier row's token
        // beat a later row's prefix, so pin the precedence in both table directions:
        // P1S (row 8) and H2D (row 10) sit on opposite sides of each other.
        assert_eq!(
            resolve_model("01P123456789", Some("H2D")),
            PrinterModel::P1S
        );
        assert_eq!(
            resolve_model("094123456789", Some("P1S")),
            PrinterModel::H2D
        );
        // An unrecognized dev_model must not suppress a good prefix either.
        assert_eq!(
            resolve_model("01P123456789", Some("FUTURE")),
            PrinterModel::P1S
        );
    }

    #[test]
    fn test_supported_models_covers_every_variant_but_unknown() {
        // Adding a PrinterModel variant makes this match non-exhaustive and fails to
        // compile, which is what forces both VARIANTS below and the MODELS table to be
        // updated -- nothing else keeps the table in sync with the enum.
        fn is_supported(model: PrinterModel) -> bool {
            match model {
                PrinterModel::X1C
                | PrinterModel::X1E
                | PrinterModel::X2D
                | PrinterModel::A1Mini
                | PrinterModel::A1
                | PrinterModel::A2L
                | PrinterModel::P1P
                | PrinterModel::P1S
                | PrinterModel::P2S
                | PrinterModel::H2D
                | PrinterModel::H2DPro
                | PrinterModel::H2C
                | PrinterModel::H2S => true,
                PrinterModel::Unknown => false,
            }
        }

        const VARIANTS: &[PrinterModel] = &[
            PrinterModel::X1C,
            PrinterModel::X1E,
            PrinterModel::X2D,
            PrinterModel::A1Mini,
            PrinterModel::A1,
            PrinterModel::A2L,
            PrinterModel::P1P,
            PrinterModel::P1S,
            PrinterModel::P2S,
            PrinterModel::H2D,
            PrinterModel::H2DPro,
            PrinterModel::H2C,
            PrinterModel::H2S,
            PrinterModel::Unknown,
        ];

        for &model in VARIANTS {
            assert_eq!(
                is_supported(model),
                supported_models().any(|m| m == model),
                "{model:?} membership in supported_models() disagrees with the enum"
            );
        }
        assert_eq!(MODELS.len(), VARIANTS.len() - 1);
    }

    #[test]
    fn test_every_supported_model_round_trips_through_its_own_signals() {
        for model in supported_models() {
            let prefix = model.serial_prefix().expect("supported model has a prefix");
            assert_eq!(resolve_model(&format!("{prefix}123456789"), None), model);

            let spec = model.spec().expect("supported model has a row");
            for token in spec.dev_tokens {
                assert_eq!(
                    resolve_model("999000000", Some(token)),
                    model,
                    "dev token {token:?} resolved to the wrong model"
                );
            }
        }
    }

    #[test]
    fn test_table_has_no_duplicate_prefixes_or_tokens() {
        for (i, spec) in MODELS.iter().enumerate() {
            for other in &MODELS[i + 1..] {
                assert!(
                    !spec.serial_prefix.eq_ignore_ascii_case(other.serial_prefix),
                    "duplicate serial prefix {:?}",
                    spec.serial_prefix
                );
                for token in spec.dev_tokens {
                    assert!(
                        !other
                            .dev_tokens
                            .iter()
                            .any(|t| token.eq_ignore_ascii_case(t)),
                        "dev token {token:?} claimed by two models"
                    );
                }
            }
        }
    }

    #[test]
    fn test_display_name_and_display_impl() {
        assert_eq!(PrinterModel::H2DPro.display_name(), "H2D Pro");
        // MODEL_MATRIX.csv and Bambu's own naming use a lowercase "mini" -- distinct from
        // the "A1 Mini" *wire token*, which is why display_name is never matched against.
        assert_eq!(PrinterModel::A1Mini.display_name(), "A1 mini");
        assert_eq!(PrinterModel::X1C.display_name(), "X1C");
        assert_eq!(PrinterModel::Unknown.display_name(), UNKNOWN_DISPLAY_NAME);

        for model in supported_models() {
            assert_eq!(format!("{model}"), model.display_name());
            assert!(!model.display_name().is_empty());
        }
        assert_eq!(format!("{}", PrinterModel::Unknown), "Unknown");
    }

    #[test]
    fn test_unknown_has_no_serial_prefix() {
        assert_eq!(PrinterModel::Unknown.serial_prefix(), None);
        assert_eq!(PrinterModel::H2C.serial_prefix(), Some("31B"));
        assert!(!supported_models().any(|m| m == PrinterModel::Unknown));
    }
}
