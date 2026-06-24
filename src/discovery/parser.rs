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

use crate::models::{BambuModel, resolve_model};

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
    a.eq_ignore_ascii_case(b)
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
