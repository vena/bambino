//! # Zero-Copy HTTP-style SSDP Parsing Engine
//!
//! Provides utilities to parse HTTP-like headers from multicast and unicast
//! UDP frames on Port 2021 without performing runtime memory allocations.
//! Differentiates Bambu Lab printers from general UPnP devices and resolves
//! serial prefixes, falling back to the `DevModel` SSDP header when the prefix
//! is unrecognized (see [`crate::models::resolve_model`]).

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
    /// WiFi signal strength in dBm (e.g. -43), if reported by the device.
    pub signal_dbm: Option<i32>,
    /// Cloud binding state (e.g. "bound", "free").
    pub bind_state: String,
    /// Security link state (e.g. "secure").
    pub security_link: String,
}

/// Evaluates equality between two standard ASCII string slices case-insensitively.
fn eq_case_insensitive(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Parses the host IP address and communication port from a LOCATION URI.
///
/// Handles both full URIs (`http://192.168.1.150:80/`) and bare IPs (`192.168.1.158`)
/// as documented in [REF-NET-DISC] Protocol Violation #3.
fn parse_location(loc: &str) -> Option<(&str, u16)> {
    let without_proto = if let Some(stripped) = loc.strip_prefix("http://") {
        stripped
    } else if let Some(stripped) = loc.strip_prefix("https://") {
        stripped
    } else {
        loc
    };

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

/// Raw header values extracted from an SSDP packet before post-processing.
struct RawSsdpHeaders<'a> {
    usn: Option<&'a str>,
    location: Option<&'a str>,
    dev_name: Option<&'a str>,
    dev_model: Option<&'a str>,
    dev_connect: Option<&'a str>,
    dev_version: Option<&'a str>,
    dev_signal: Option<&'a str>,
    dev_bind: Option<&'a str>,
    dev_seclink: Option<&'a str>,
    nt_or_st: Option<&'a str>,
}

/// Extracts SSDP header values from a parsed header slice.
///
/// Only bails (`?`) on UTF-8 decode failure for required headers (USN, LOCATION).
/// Optional headers with non-UTF-8 values are silently skipped per [REF-NET-DISC].
fn extract_headers<'a>(headers: &[httparse::Header<'a>]) -> Option<RawSsdpHeaders<'a>> {
    let mut raw = RawSsdpHeaders {
        usn: None,
        location: None,
        dev_name: None,
        dev_model: None,
        dev_connect: None,
        dev_version: None,
        dev_signal: None,
        dev_bind: None,
        dev_seclink: None,
        nt_or_st: None,
    };

    for header in headers {
        let name = header.name;

        if eq_case_insensitive(name, "usn") {
            raw.usn = Some(core::str::from_utf8(header.value).ok()?);
        } else if eq_case_insensitive(name, "location") {
            raw.location = Some(core::str::from_utf8(header.value).ok()?);
        } else {
            let Some(value_str) = core::str::from_utf8(header.value).ok() else {
                continue;
            };

            if eq_case_insensitive(name, "devname.bambu.com")
                || eq_case_insensitive(name, "devname")
            {
                raw.dev_name = Some(value_str);
            } else if eq_case_insensitive(name, "devmodel.bambu.com")
                || eq_case_insensitive(name, "devmodel")
            {
                raw.dev_model = Some(value_str);
            } else if eq_case_insensitive(name, "devconnect.bambu.com")
                || eq_case_insensitive(name, "devconnect")
            {
                raw.dev_connect = Some(value_str);
            } else if eq_case_insensitive(name, "devversion.bambu.com")
                || eq_case_insensitive(name, "devversion")
            {
                raw.dev_version = Some(value_str);
            } else if eq_case_insensitive(name, "devsignal.bambu.com")
                || eq_case_insensitive(name, "devsignal")
            {
                raw.dev_signal = Some(value_str);
            } else if eq_case_insensitive(name, "devbind.bambu.com")
                || eq_case_insensitive(name, "devbind")
            {
                raw.dev_bind = Some(value_str);
            } else if eq_case_insensitive(name, "devseclink.bambu.com")
                || eq_case_insensitive(name, "devseclink")
            {
                raw.dev_seclink = Some(value_str);
            } else if eq_case_insensitive(name, "nt") || eq_case_insensitive(name, "st") {
                raw.nt_or_st = Some(value_str);
            }
        }
    }

    Some(raw)
}

/// Extracts a model identifier from an NT or ST header value.
///
/// Per [REF-NET-DISC] Protocol Violation #7, some firmware tracks embed the model
/// directly in the target URN (e.g. `urn:bambulab-com:device:P1S:1`).
fn extract_model_from_nt_st(value: &str) -> Option<&str> {
    let stripped = value.strip_prefix("urn:bambulab-com:device:")?;
    let model = stripped.split(':').next()?;
    if eq_case_insensitive(model, "3dprinter") {
        return None;
    }
    Some(model)
}

/// Parse an incoming raw UDP datagram buffer into normalized printer credentials.
///
/// Under the SSDP specification, responses map to standard HTTP responses, while
/// advertisements map to HTTP requests. This parser automatically evaluates the envelope
/// and routes the payload buffer to the appropriate parsing schema of `httparse`.
pub fn parse_ssdp_payload(buf: &[u8]) -> Option<SsdpDevice> {
    let mut headers = [httparse::EMPTY_HEADER; 32];

    let is_response = buf.starts_with(b"HTTP/") || buf.starts_with(b"http/");

    let raw = if is_response {
        let mut response = httparse::Response::new(&mut headers);
        response.parse(buf).ok()?;
        extract_headers(response.headers)?
    } else {
        let mut request = httparse::Request::new(&mut headers);
        request.parse(buf).ok()?;
        extract_headers(request.headers)?
    };

    let raw_usn_str = raw.usn?;
    let serial = raw_usn_str.strip_prefix("uuid:").unwrap_or(raw_usn_str);

    // Use DevModel header, falling back to model embedded in NT/ST per Protocol Violation #7
    let effective_dev_model = raw
        .dev_model
        .or_else(|| raw.nt_or_st.and_then(extract_model_from_nt_st));

    let (ip, port) = raw.location.and_then(parse_location)?;
    let model = resolve_model(serial, effective_dev_model);

    let signal_dbm = raw.dev_signal.and_then(|s| s.parse::<i32>().ok());

    Some(SsdpDevice {
        serial: serial.to_owned(),
        model,
        name: raw.dev_name.unwrap_or("").to_owned(),
        ip: ip.to_owned(),
        port,
        discovery_port: 0,
        version: raw.dev_version.unwrap_or("").to_owned(),
        connect_type: raw.dev_connect.unwrap_or("").to_owned(),
        raw_model_str: effective_dev_model.unwrap_or("").to_owned(),
        signal_dbm,
        bind_state: raw.dev_bind.unwrap_or("").to_owned(),
        security_link: raw.dev_seclink.unwrap_or("").to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_location_uri() {
        let (ip, port) = parse_location("http://192.168.1.150:80/").unwrap();
        assert_eq!(ip, "192.168.1.150");
        assert_eq!(port, 80);

        let (ip2, port2) = parse_location("https://10.0.0.42:8080/path").unwrap();
        assert_eq!(ip2, "10.0.0.42");
        assert_eq!(port2, 8080);
    }

    #[test]
    fn test_parse_location_bare_ip() {
        let (ip, port) = parse_location("192.168.1.158").unwrap();
        assert_eq!(ip, "192.168.1.158");
        assert_eq!(port, 80);
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
                        USN: uuid:09306A521703533\r\n\
                        DevName.bambu.com: MyPrinterName\r\n\
                        DevModel.bambu.com: O1S\r\n\
                        DevConnect.bambu.com: lan\r\n\
                        DevVersion.bambu.com: 01.02.00.00\r\n\r\n";

        let device = parse_ssdp_payload(payload).unwrap();
        assert_eq!(device.serial, "09306A521703533");
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

    #[test]
    fn test_non_utf8_optional_header_does_not_discard_packet() {
        let mut payload = b"HTTP/1.1 200 OK\r\n\
                            LOCATION: http://10.0.0.5:80/\r\n\
                            USN: 01P06A521703222\r\n\
                            DevModel.bambu.com: C12\r\n\
                            DevSignal.bambu.com: "
            .to_vec();
        payload.extend_from_slice(&[0xFF, 0xFE]);
        payload.extend_from_slice(b"\r\n\r\n");

        let device = parse_ssdp_payload(&payload).unwrap();
        assert_eq!(device.serial, "01P06A521703222");
        assert_eq!(device.model, BambuModel::P1S);
        assert!(device.signal_dbm.is_none());
    }

    #[test]
    fn test_signal_bind_seclink_fields_extracted() {
        let payload = b"NOTIFY * HTTP/1.1\r\n\
                        HOST: 239.255.255.250:2021\r\n\
                        LOCATION: http://192.168.1.150:80/\r\n\
                        USN: 09306A521703533\r\n\
                        DevModel.bambu.com: O1S\r\n\
                        DevSignal.bambu.com: -43\r\n\
                        DevBind.bambu.com: bound\r\n\
                        Devseclink.bambu.com: secure\r\n\r\n";

        let device = parse_ssdp_payload(payload).unwrap();
        assert_eq!(device.signal_dbm, Some(-43));
        assert_eq!(device.bind_state, "bound");
        assert_eq!(device.security_link, "secure");
    }

    #[test]
    fn test_nt_st_fallback_model_resolution() {
        let payload = b"NOTIFY * HTTP/1.1\r\n\
                        HOST: 239.255.255.250:2021\r\n\
                        LOCATION: http://192.168.1.42:80/\r\n\
                        USN: 01P06A521703222\r\n\
                        NT: urn:bambulab-com:device:C12:1\r\n\r\n";

        let device = parse_ssdp_payload(payload).unwrap();
        assert_eq!(device.model, BambuModel::P1S);
        assert_eq!(device.raw_model_str, "C12");
    }

    #[test]
    fn test_nt_generic_3dprinter_not_used_as_model() {
        let payload = b"NOTIFY * HTTP/1.1\r\n\
                        HOST: 239.255.255.250:2021\r\n\
                        LOCATION: http://192.168.1.42:80/\r\n\
                        USN: 01P06A521703222\r\n\
                        NT: urn:bambulab-com:device:3dprinter:1\r\n\r\n";

        let device = parse_ssdp_payload(payload).unwrap();
        assert_eq!(device.model, BambuModel::P1S);
        assert_eq!(device.raw_model_str, "");
    }

    #[test]
    fn test_extract_model_from_nt_st() {
        assert_eq!(
            extract_model_from_nt_st("urn:bambulab-com:device:P1S:1"),
            Some("P1S")
        );
        assert_eq!(
            extract_model_from_nt_st("urn:bambulab-com:device:3dprinter:1"),
            None
        );
        assert_eq!(extract_model_from_nt_st("ssdp:alive"), None);
    }

    #[test]
    fn test_p1s_real_notify_bare_location() {
        let payload = b"NOTIFY * HTTP/1.1\r\n\
                        HOST: 239.255.255.250:1900\r\n\
                        Server: UPnP/1.0\r\n\
                        Location: 192.168.1.158\r\n\
                        NT: urn:bambulab-com:device:3dprinter:1\r\n\
                        USN: 01P00A4C2009981\r\n\
                        Cache-Control: max-age=1800\r\n\
                        DevModel.bambu.com: C12\r\n\
                        DevName.bambu.com: 3DP-01P-981\r\n\
                        DevSignal.bambu.com: -43\r\n\
                        DevConnect.bambu.com: lan\r\n\
                        DevBind.bambu.com: free\r\n\
                        Devseclink.bambu.com: secure\r\n\
                        DevVersion.bambu.com: 01.10.00.00\r\n\
                        DevCap.bambu.com: 1\r\n\r\n";

        let device = parse_ssdp_payload(payload).unwrap();
        assert_eq!(device.serial, "01P00A4C2009981");
        assert_eq!(device.model, BambuModel::P1S);
        assert_eq!(device.ip, "192.168.1.158");
        assert_eq!(device.port, 80);
        assert_eq!(device.name, "3DP-01P-981");
        assert_eq!(device.signal_dbm, Some(-43));
        assert_eq!(device.bind_state, "free");
        assert_eq!(device.security_link, "secure");
        assert_eq!(device.version, "01.10.00.00");
        assert_eq!(device.connect_type, "lan");
    }
}
