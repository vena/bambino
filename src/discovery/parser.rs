//! # Zero-Copy HTTP-style SSDP Parsing Engine
//!
//! Provides utilities to parse HTTP-like headers from multicast and unicast
//! UDP frames on Port 2021 without performing runtime memory allocations.
//! Differentiates Bambu Lab printers from general UPnP devices, resolves
//! serial prefixes, and bypasses the H2S/H2D collision hazard.

#[cfg(not(feature = "std"))]
use alloc::borrow::ToOwned;
#[cfg(not(feature = "std"))]
use alloc::string::String;

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
    /// H2C (Vortek hotend tool-changer platform)
    H2C,
    /// H2S (Single-nozzle platform sharing H2 mechanics)
    H2S,
    /// Fallback variant for newly released or unrecognized printer targets
    Unknown,
}

/// Normalized device details extracted directly from SSDP UDP datagram payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsdpDevice {
    /// The unique uppercase physical hardware serial number.
    pub serial: String,
    /// Resolved printer capability profile based on prefixes and headers.
    pub model: BambuModel,
    /// Human-friendly printer name defined by the user.
    pub name: String,
    /// Direct network target IP address extracted from the LOCATION header.
    pub ip: String,
    /// Discovery communications port parsed from the LOCATION header.
    pub port: u16,
    /// SSDP port on which the device was discovered (2021 or 1990).
    pub discovery_port: u16,
    /// Device firmware target version.
    pub version: String,
    /// Network connection medium (e.g. "lan", "wlan").
    pub connect_type: String,
    /// Unmodified hardware identifier returned by the network card.
    pub raw_model_str: String,
}

/// Evaluates equality between two standard ASCII string slices case-insensitively.
///
/// **Why this is used:** Bypasses heap-allocation overhead of `to_ascii_lowercase()`
/// or standard matching in constraint-heavy embedded microcontrollers.
fn eq_case_insensitive(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .all(|(char_a, char_b)| char_a.to_ascii_lowercase() == char_b.to_ascii_lowercase())
}

/// Resolves the specific printer model using physical serial number prefixes combined
/// with target SSDP model advertisements to bypass collision signatures.
///
/// **Why prefix checks alone are insufficient:**
/// The single-nozzle `H2S` and dual-nozzle `H2D` share the identical hardware label
/// serial prefix `094`. We must query the optional `DevModel` header to resolve
/// which printer model is active before routing commands `[REF-NET-PORTS]`.
pub fn resolve_model(serial: &str, dev_model: Option<&str>) -> BambuModel {
    let prefix = if serial.len() >= 3 { &serial[0..3] } else { "" };

    match prefix {
        "094" => match dev_model {
            Some(m) if m.contains("O1S") => BambuModel::H2S,
            Some(m) if m.contains("O1D") => BambuModel::H2D,
            Some(m) if m.contains("O1E") || m.contains("O2D") => BambuModel::H2DPro,
            Some(m) if m.contains("O1C") || m.contains("O1C2") => BambuModel::H2C,
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
            // Direct header string fallback if serial is missing or unrecognizable
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

/// Parses the host IP address and communication port from a LOCATION URI.
/// Handles protocols seamlessly and performs zero memory allocations.
///
/// **Format expected:** `http://192.168.1.150:80/`
fn parse_location(loc: &str) -> Option<(&str, u16)> {
    let without_proto = if let Some(stripped) = loc.strip_prefix("http://") {
        stripped
    } else if let Some(stripped) = loc.strip_prefix("https://") {
        stripped
    } else {
        loc
    };

    // Extract the host portion prior to the first path directory slash
    let host_port = without_proto.split('/').next()?;

    let mut parts = host_port.split(':');
    let host = parts.next()?;
    let port = if let Some(port_str) = parts.next() {
        port_str.parse::<u16>().ok().unwrap_or(80)
    } else {
        80
    };

    Some((host, port))
}

/// Parse an incoming raw UDP datagram buffer into normalized printer credentials.
///
/// Under the SSDP specification, responses map to standard HTTP responses, while
/// advertisements map to HTTP requests. This parser automatically evaluates the envelope
/// and routes the payload buffer to the appropriate parsing schema of `httparse`.
pub fn parse_ssdp_payload(buf: &[u8]) -> Option<SsdpDevice> {
    let mut headers = [httparse::EMPTY_HEADER; 32];

    // Determine the packet formatting by inspecting the starting bytes
    let is_response = buf.starts_with(b"HTTP/") || buf.starts_with(b"http/");

    let mut raw_usn = None;
    let mut location = None;
    let mut dev_name = None;
    let mut dev_model = None;
    let mut dev_connect = None;
    let mut dev_version = None;

    if is_response {
        let mut response = httparse::Response::new(&mut headers);
        response.parse(buf).ok()?;

        for header in response.headers {
            let value_str = core::str::from_utf8(header.value).ok()?;
            if eq_case_insensitive(header.name, "usn") {
                raw_usn = Some(value_str);
            } else if eq_case_insensitive(header.name, "location") {
                location = Some(value_str);
            } else if eq_case_insensitive(header.name, "devname.bambu.com")
                || eq_case_insensitive(header.name, "devname")
            {
                dev_name = Some(value_str);
            } else if eq_case_insensitive(header.name, "devmodel.bambu.com")
                || eq_case_insensitive(header.name, "devmodel")
            {
                dev_model = Some(value_str);
            } else if eq_case_insensitive(header.name, "devconnect.bambu.com")
                || eq_case_insensitive(header.name, "devconnect")
            {
                dev_connect = Some(value_str);
            } else if eq_case_insensitive(header.name, "devversion.bambu.com")
                || eq_case_insensitive(header.name, "devversion")
            {
                dev_version = Some(value_str);
            }
        }
    } else {
        let mut request = httparse::Request::new(&mut headers);
        request.parse(buf).ok()?;

        for header in request.headers {
            let value_str = core::str::from_utf8(header.value).ok()?;
            if eq_case_insensitive(header.name, "usn") {
                raw_usn = Some(value_str);
            } else if eq_case_insensitive(header.name, "location") {
                location = Some(value_str);
            } else if eq_case_insensitive(header.name, "devname.bambu.com")
                || eq_case_insensitive(header.name, "devname")
            {
                dev_name = Some(value_str);
            } else if eq_case_insensitive(header.name, "devmodel.bambu.com")
                || eq_case_insensitive(header.name, "devmodel")
            {
                dev_model = Some(value_str);
            } else if eq_case_insensitive(header.name, "devconnect.bambu.com")
                || eq_case_insensitive(header.name, "devconnect")
            {
                dev_connect = Some(value_str);
            } else if eq_case_insensitive(header.name, "devversion.bambu.com")
                || eq_case_insensitive(header.name, "devversion")
            {
                dev_version = Some(value_str);
            }
        }
    }

    // Extraction processing: Bypasses the bare USN vs UUID deviations defined in [REF-NET-DISC]
    let raw_usn_str = raw_usn?;
    let serial = if let Some(stripped) = raw_usn_str.strip_prefix("uuid:") {
        stripped
    } else {
        raw_usn_str
    };

    let (ip, port) = location.and_then(parse_location)?;
    let model = resolve_model(serial, dev_model);

    Some(SsdpDevice {
        serial: serial.to_owned(),
        model,
        name: dev_name.unwrap_or("").to_owned(),
        ip: ip.to_owned(),
        port,
        discovery_port: 0,
        version: dev_version.unwrap_or("").to_owned(),
        connect_type: dev_connect.unwrap_or("").to_owned(),
        raw_model_str: dev_model.unwrap_or("").to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_location_uri() {
        let loc1 = "http://192.168.1.150:80/";
        let (ip, port) = parse_location(loc1).unwrap();
        assert_eq!(ip, "192.168.1.150");
        assert_eq!(port, 80);

        let loc2 = "https://10.0.0.42:8080/path";
        let (ip2, port2) = parse_location(loc2).unwrap();
        assert_eq!(ip2, "10.0.0.42");
        assert_eq!(port2, 8080);
    }

    #[test]
    fn test_case_insensitive_matching() {
        assert!(eq_case_insensitive("LOCATION", "location"));
        assert!(eq_case_insensitive(
            "devname.bambu.com",
            "DevName.bambu.com"
        ));
        assert!(!eq_case_insensitive("devmodel", "devversion"));
    }

    #[test]
    fn test_h2_collision_resolution() {
        // H2S single nozzle configuration (O1S)
        let model1 = resolve_model("09406A521703533", Some("O1S"));
        assert_eq!(model1, BambuModel::H2S);

        // H2D dual nozzle configuration (O1D)
        let model2 = resolve_model("09406A521703533", Some("O1D"));
        assert_eq!(model2, BambuModel::H2D);

        // Fallback default for 094 serial with missing dev_model
        let model3 = resolve_model("09406A521703533", None);
        assert_eq!(model3, BambuModel::H2S);
    }

    #[test]
    fn test_typical_model_prefix_resolution() {
        assert_eq!(resolve_model("00M123456789", None), BambuModel::X1C);
        assert_eq!(resolve_model("01P123456789", None), BambuModel::P1S);
        assert_eq!(resolve_model("039123456789", None), BambuModel::A1);
        assert_eq!(resolve_model("999123456789", Some("C12")), BambuModel::P1S);
        assert_eq!(resolve_model("999123456789", None), BambuModel::Unknown);
    }

    #[test]
    fn test_parse_ssdp_notify_packet() {
        let payload = b"NOTIFY * HTTP/1.1\r\n\
                        HOST: 239.255.255.250:2021\r\n\
                        LOCATION: http://192.168.1.150:80/\r\n\
                        USN: uuid:09406A521703533\r\n\
                        DevName.bambu.com: MyPrinterName\r\n\
                        DevModel.bambu.com: O1S\r\n\
                        DevConnect.bambu.com: lan\r\n\
                        DevVersion.bambu.com: 01.02.00.00\r\n\r\n";

        let device = parse_ssdp_payload(payload).unwrap();
        assert_eq!(device.serial, "09406A521703533");
        assert_eq!(device.model, BambuModel::H2S);
        assert_eq!(device.ip, "192.168.1.150");
        assert_eq!(device.port, 80);
        assert_eq!(device.name, "MyPrinterName");
        assert_eq!(device.version, "01.02.00.00");
    }

    #[test]
    fn test_parse_ssdp_search_reply_with_bare_usn() {
        let payload = b"HTTP/1.1 200 OK\r\n\
                        LOCATION: http://10.0.0.5:80/\r\n\
                        USN: 01P06A521703222\r\n\
                        DevModel.bambu.com: C12\r\n\r\n";

        let device = parse_ssdp_payload(payload).unwrap();
        assert_eq!(device.serial, "01P06A521703222");
        assert_eq!(device.model, BambuModel::P1S);
        assert_eq!(device.ip, "10.0.0.5");
        assert_eq!(device.port, 80);
    }
}
