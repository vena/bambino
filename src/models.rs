//! # Printer Model Identification
//!
//! Every Bambu Lab printer has a 3-character serial number prefix that identifies
//! its model. [`PrinterModel`] enumerates all known models, and [`resolve_model()`]
//! maps serial prefixes (with an SSDP `DevModel` fallback) to the right variant.
//! The resolved model drives behavioral dispatch through the [`crate::quirks`] engine.

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

/// Resolves the specific printer model using physical serial number prefixes combined with target SSDP model advertisements as a secondary signal.
///
/// Each H2-series model has a distinct serial prefix confirmed by the Bambu Lab wiki:
/// `094` = H2D, `093` = H2S, `239` = H2D Pro, `31B` = H2C. When the prefix is
/// unrecognized, the optional `DevModel` SSDP header provides a fallback path.
pub fn resolve_model(serial: &str, dev_model: Option<&str>) -> PrinterModel {
    let prefix = serial.get(0..3).unwrap_or("");

    match prefix {
        "094" => PrinterModel::H2D,
        "093" => PrinterModel::H2S,
        "239" => PrinterModel::H2DPro,
        "31B" => PrinterModel::H2C,
        "00M" => PrinterModel::X1C,
        "03W" => PrinterModel::X1E,
        "20P" => PrinterModel::X2D,
        "01S" => PrinterModel::P1P,
        "01P" => PrinterModel::P1S,
        "22E" => PrinterModel::P2S,
        "030" => PrinterModel::A1Mini,
        "039" => PrinterModel::A1,
        "26A" => PrinterModel::A2L,
        _ => {
            if let Some(m) = dev_model {
                match m {
                    "BL-P001" => PrinterModel::X1C,
                    "C13" => PrinterModel::X1E,
                    "N6" => PrinterModel::X2D,
                    "N1" => PrinterModel::A1Mini,
                    "N2S" => PrinterModel::A1,
                    "N9" => PrinterModel::A2L,
                    "C11" => PrinterModel::P1P,
                    "C12" => PrinterModel::P1S,
                    "N7" => PrinterModel::P2S,
                    "O1D" => PrinterModel::H2D,
                    "O1E" | "O2D" => PrinterModel::H2DPro,
                    "O1C" | "O1C2" => PrinterModel::H2C,
                    "O1S" => PrinterModel::H2S,
                    _ => PrinterModel::Unknown,
                }
            } else {
                PrinterModel::Unknown
            }
        }
    }
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
        assert_eq!(resolve_model("999000000", Some("O1E")), PrinterModel::H2DPro);
        assert_eq!(resolve_model("999000000", Some("O2D")), PrinterModel::H2DPro);
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
        assert_eq!(resolve_model("999000000", Some("BL-P001")), PrinterModel::X1C);
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
}
