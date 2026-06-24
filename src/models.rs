//! # Canonical Printer Model Identity
//!
//! Defines the `BambuModel` enum representing all supported Bambu Lab printer
//! hardware variants, and `resolve_model()` for mapping serial prefixes and
//! SSDP device model strings to the correct variant.

/// Enumeration of physical Bambu Lab printer models supported on the local interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BambuModel {
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

/// Resolves the specific printer model using physical serial number prefixes combined
/// with target SSDP model advertisements to bypass collision signatures.
///
/// **Why prefix checks alone are insufficient:**
/// The single-nozzle `H2S` and dual-nozzle `H2D` share the identical hardware label
/// serial prefix `094`. We must query the optional `DevModel` header to resolve
/// which printer model is active before routing commands `[REF-NET-PORTS]`.
pub fn resolve_model(serial: &str, dev_model: Option<&str>) -> BambuModel {
    let prefix = serial.get(0..3).unwrap_or("");

    match prefix {
        "094" => match dev_model {
            Some("O1S") => BambuModel::H2S,
            Some("O1D") => BambuModel::H2D,
            Some("O1E") | Some("O2D") => BambuModel::H2DPro,
            Some("O1C") | Some("O1C2") => BambuModel::H2C,
            _ => BambuModel::H2S, // Safe default fallback
        },
        "00M" => BambuModel::X1C,
        "03W" => BambuModel::X1E,
        "20P" => BambuModel::X2D,
        "01S" => BambuModel::P1P,
        "01P" => BambuModel::P1S,
        "22E" => BambuModel::P2S,
        "030" => BambuModel::A1Mini,
        "039" => BambuModel::A1,
        "26A" => BambuModel::A2L,
        _ => {
            if let Some(m) = dev_model {
                match m {
                    "BL-P001" => BambuModel::X1C,
                    "C13" => BambuModel::X1E,
                    "N6" => BambuModel::X2D,
                    "N1" => BambuModel::A1Mini,
                    "N2S" => BambuModel::A1,
                    "N9" => BambuModel::A2L,
                    "C11" => BambuModel::P1P,
                    "C12" => BambuModel::P1S,
                    "N7" => BambuModel::P2S,
                    "O1D" => BambuModel::H2D,
                    "O1E" | "O2D" => BambuModel::H2DPro,
                    "O1C" | "O1C2" => BambuModel::H2C,
                    "O1S" => BambuModel::H2S,
                    _ => BambuModel::Unknown,
                }
            } else {
                BambuModel::Unknown
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_h2_collision_resolution() {
        assert_eq!(
            resolve_model("09406A521703533", Some("O1S")),
            BambuModel::H2S
        );
        assert_eq!(
            resolve_model("09406A521703533", Some("O1D")),
            BambuModel::H2D
        );
        assert_eq!(
            resolve_model("09406A521703533", Some("O1E")),
            BambuModel::H2DPro
        );
        assert_eq!(
            resolve_model("09406A521703533", Some("O2D")),
            BambuModel::H2DPro
        );
        assert_eq!(
            resolve_model("09406A521703533", Some("O1C")),
            BambuModel::H2C
        );
        assert_eq!(
            resolve_model("09406A521703533", Some("O1C2")),
            BambuModel::H2C
        );
        assert_eq!(resolve_model("09406A521703533", None), BambuModel::H2S);
    }

    #[test]
    fn test_all_prefix_resolution() {
        assert_eq!(resolve_model("00M123456789", None), BambuModel::X1C);
        assert_eq!(resolve_model("03W123456789", None), BambuModel::X1E);
        assert_eq!(resolve_model("20P123456789", None), BambuModel::X2D);
        assert_eq!(resolve_model("01S123456789", None), BambuModel::P1P);
        assert_eq!(resolve_model("01P123456789", None), BambuModel::P1S);
        assert_eq!(resolve_model("22E123456789", None), BambuModel::P2S);
        assert_eq!(resolve_model("030123456789", None), BambuModel::A1Mini);
        assert_eq!(resolve_model("039123456789", None), BambuModel::A1);
        assert_eq!(resolve_model("26A123456789", None), BambuModel::A2L);
    }

    #[test]
    fn test_dev_model_fallback() {
        assert_eq!(resolve_model("999000000", Some("BL-P001")), BambuModel::X1C);
        assert_eq!(resolve_model("999000000", Some("C13")), BambuModel::X1E);
        assert_eq!(resolve_model("999000000", Some("N6")), BambuModel::X2D);
        assert_eq!(resolve_model("999000000", Some("N1")), BambuModel::A1Mini);
        assert_eq!(resolve_model("999000000", Some("N2S")), BambuModel::A1);
        assert_eq!(resolve_model("999000000", Some("N9")), BambuModel::A2L);
        assert_eq!(resolve_model("999000000", Some("C11")), BambuModel::P1P);
        assert_eq!(resolve_model("999000000", Some("C12")), BambuModel::P1S);
        assert_eq!(resolve_model("999000000", Some("N7")), BambuModel::P2S);
        assert_eq!(resolve_model("999000000", Some("O1D")), BambuModel::H2D);
        assert_eq!(resolve_model("999000000", Some("O1E")), BambuModel::H2DPro);
        assert_eq!(resolve_model("999000000", Some("O2D")), BambuModel::H2DPro);
        assert_eq!(resolve_model("999000000", Some("O1C")), BambuModel::H2C);
        assert_eq!(resolve_model("999000000", Some("O1C2")), BambuModel::H2C);
        assert_eq!(resolve_model("999000000", Some("O1S")), BambuModel::H2S);
        assert_eq!(
            resolve_model("999000000", Some("FUTURE")),
            BambuModel::Unknown
        );
        assert_eq!(resolve_model("999000000", None), BambuModel::Unknown);
    }

    #[test]
    fn test_short_and_empty_serial() {
        assert_eq!(resolve_model("", None), BambuModel::Unknown);
        assert_eq!(resolve_model("AB", None), BambuModel::Unknown);
        assert_eq!(resolve_model("00", None), BambuModel::Unknown);
        assert_eq!(resolve_model("", Some("C12")), BambuModel::P1S);
    }
}
