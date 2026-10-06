//! # HTTP-style SSDP Parsing
//!
//! Parses HTTP-like headers from multicast and unicast UDP frames on port 2021. Header
//! slicing is zero-copy and a non-Bambu packet is rejected without allocating; an accepted
//! packet allocates the owned [`SsdpDevice`] it returns.
//! Differentiates Bambu Lab printers from general UPnP devices and resolves
//! serial prefixes, falling back to the `DevModel` SSDP header when the prefix
//! is unrecognized (see [`crate::models::resolve_model`]).

#[cfg(not(feature = "std"))]
use alloc::borrow::ToOwned;
#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use core::net::{IpAddr, SocketAddr};

use crate::models::{PrinterModel, resolve_model};

/// Header slots `httparse` gets per SSDP packet. A packet with more headers than this fails to
/// parse and is dropped silently; current Bambu packets carry about 15, so 32 leaves headroom.
pub(crate) const SSDP_MAX_HEADERS: usize = 32;

/// Normalized device details extracted directly from SSDP UDP datagram payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsdpDevice {
    /// The unique uppercase physical hardware serial number.
    pub serial: String,
    /// Resolved printer capability profile based on prefixes and headers.
    pub model: PrinterModel,
    /// Human-friendly printer name defined by the user.
    pub name: String,
    /// Printer IP address from the LOCATION header. A packet whose LOCATION host isn't an IP
    /// literal is rejected, so this is always safe to dial or interpolate into a URL.
    pub ip: IpAddr,
    /// Port of the LOCATION URI (80 when absent). This is an inert HTTP endpoint, **not** the
    /// MQTT, FTPS or camera port — see [REF-NET-DISC] Protocol Violation #2.
    pub location_port: u16,
    /// SSDP port on which the device was discovered (2021 or 1990), or `None` if unknown.
    ///
    /// The port is not carried in the payload, so [`parse_ssdp_payload`] — which sees only the
    /// datagram bytes — always leaves this `None`. It is filled in by
    /// [`DiscoveryEngine::poll_next_device`](crate::discovery::DiscoveryEngine::poll_next_device),
    /// which knows which socket the datagram arrived on.
    pub discovery_port: Option<u16>,
    /// Device firmware target version.
    pub version: String,
    /// Network connection medium (e.g. "lan", "wlan").
    pub connect_type: String,
    /// Hardware identifier from the `DevModel.bambu.com` header, or the NT/ST URN-derived fallback string when that header is absent/empty (see `effective_dev_model`).
    pub raw_model_str: String,
    /// WiFi signal strength in dBm (e.g. -43), if reported by the device.
    pub signal_dbm: Option<i32>,
    /// Cloud binding state (e.g. "bound", "free").
    pub bind_state: String,
    /// Security link state (e.g. "secure").
    pub security_link: String,
}

impl SsdpDevice {
    /// The identity to connect to this printer with, keeping the model discovery resolved.
    ///
    /// Unlike `PrinterIdentity::new(dev.ip, dev.serial, ..)`, which re-resolves the model from
    /// the serial alone, this keeps [`model`](Self::model) — resolved from the serial *and* the
    /// `DevModel`/NT/ST headers — so a printer with an unrecognized serial prefix doesn't fall
    /// back to the conservative `Unknown` quirks.
    #[must_use]
    pub fn into_identity(self, access_code: impl Into<String>) -> crate::identity::PrinterIdentity {
        crate::identity::PrinterIdentity {
            ip: self.ip.to_string(),
            serial: self.serial,
            access_code: access_code.into(),
            model: self.model,
        }
    }
}

/// Domain suffix on Bambu's vendor SSDP headers (`DevName.bambu.com`); some firmware omits it.
const BAMBU_HEADER_SUFFIX: &str = ".bambu.com";

/// Strips an optional, case-insensitive [`BAMBU_HEADER_SUFFIX`] from a header name.
fn strip_bambu_suffix(name: &str) -> &str {
    name.len()
        .checked_sub(BAMBU_HEADER_SUFFIX.len())
        .and_then(|split| name.split_at_checked(split))
        .filter(|(_, suffix)| suffix.eq_ignore_ascii_case(BAMBU_HEADER_SUFFIX))
        .map_or(name, |(short, _)| short)
}

/// Parses the host IP address and port from a LOCATION URI.
///
/// Handles both full URIs (`http://192.168.1.150:80/`) and bare IPs (`192.168.1.158`)
/// as documented in [REF-NET-DISC] Protocol Violation #3. A host that isn't an IP literal
/// rejects the packet: no firmware is documented sending a hostname, and a spoofed one
/// would otherwise flow unvalidated into every dial and URL built from it.
fn parse_location(loc: &str) -> Option<(IpAddr, u16)> {
    // Case-insensitive scheme, like every other comparison in this file (firmware casing drift,
    // [REF-NET-DISC] Violation #5): an `HTTP://` location used to drop the printer (#328).
    let strip_scheme = |scheme: &str| {
        let (prefix, rest) = loc.split_at_checked(scheme.len())?;
        prefix.eq_ignore_ascii_case(scheme).then_some(rest)
    };
    let without_proto = strip_scheme("http://")
        .or_else(|| strip_scheme("https://"))
        .unwrap_or(loc);

    let host_port = without_proto.split('/').next()?;

    // A present-but-unparseable port string (e.g. a corrupt/truncated LOCATION
    // header) must reject the packet, not silently coerce to 80 — that's indistinguishable
    // from "no port specified" and would route to the wrong port on a real device. Both
    // parses fail on `host:garbage`, so it falls through to `None`.
    if let Ok(addr) = host_port.parse::<SocketAddr>() {
        return Some((addr.ip(), addr.port()));
    }
    let bare = host_port
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host_port);
    Some((bare.parse().ok()?, 80))
}

/// Raw header values extracted from an SSDP packet before post-processing.
#[derive(Default)]
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
    let mut raw = RawSsdpHeaders::default();

    for header in headers {
        let name = header.name;

        if name.eq_ignore_ascii_case("usn") {
            raw.usn = Some(core::str::from_utf8(header.value).ok()?);
        } else if name.eq_ignore_ascii_case("location") {
            raw.location = Some(core::str::from_utf8(header.value).ok()?);
        } else {
            let Some(value_str) = core::str::from_utf8(header.value).ok() else {
                continue;
            };

            let short = strip_bambu_suffix(name);
            let is = |key: &str| short.eq_ignore_ascii_case(key);
            let slot = if is("devname") {
                &mut raw.dev_name
            } else if is("devmodel") {
                &mut raw.dev_model
            } else if is("devconnect") {
                &mut raw.dev_connect
            } else if is("devversion") {
                &mut raw.dev_version
            } else if is("devsignal") {
                &mut raw.dev_signal
            } else if is("devbind") {
                &mut raw.dev_bind
            } else if is("devseclink") {
                &mut raw.dev_seclink
            } else if is("nt") || is("st") {
                &mut raw.nt_or_st
            } else {
                continue;
            };
            *slot = Some(value_str);
        }
    }

    Some(raw)
}

/// Case-insensitive substring search that doesn't allocate a lowercased copy.
fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

/// Extracts a model identifier from an NT or ST header value.
///
/// Per [REF-NET-DISC] Protocol Violation #7, some firmware tracks embed the model
/// directly in the target URN (e.g. `urn:bambulab-com:device:P1S:1`).
///
/// The prefix match is case-**insensitive**. `is_bambu_device` keys on the same URN and is
/// already case-insensitive, so an exact-case match here meant a differently-cased URN was
/// still accepted as a Bambu device while this fallback silently extracted nothing and the
/// printer came back as `Unknown` with an empty `raw_model_str`. [REF-NET-DISC] Violation #5
/// records this codebase's own experience with casing varying across firmware tracks.
/// The returned slice keeps the URN's original casing — only the comparison is normalized.
fn extract_model_from_nt_st(value: &str) -> Option<&str> {
    const URN_PREFIX: &str = "urn:bambulab-com:device:";
    let (prefix, stripped) = value.split_at_checked(URN_PREFIX.len())?;
    if !prefix.eq_ignore_ascii_case(URN_PREFIX) {
        return None;
    }
    let model = stripped.split(':').next()?;
    if model.eq_ignore_ascii_case("3dprinter") {
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
    let mut headers = [httparse::EMPTY_HEADER; SSDP_MAX_HEADERS];

    // Case-insensitive, consistent with this file's otherwise-thorough
    // case-insensitive header handling — a non-canonical-case status
    // line must route to the response parser, not fall through to the request parser and
    // fail there instead. Note `httparse::Response::parse` itself still requires an
    // exact-case "HTTP/" token and rejects a non-canonical-case status line regardless, so
    // this only fixes which parser rejects it — see test_lowercase_status_line_routes_to_response_parser.
    let is_response = buf.len() >= 5 && buf[..5].eq_ignore_ascii_case(b"HTTP/");

    let raw = if is_response {
        let mut response = httparse::Response::new(&mut headers);
        // httparse::Status::Partial means the buffer ended mid-header —
        // a truncated UDP datagram must be rejected, not treated the same as a
        // successfully fully-parsed packet.
        if !matches!(response.parse(buf).ok()?, httparse::Status::Complete(_)) {
            return None;
        }
        extract_headers(response.headers)?
    } else {
        let mut request = httparse::Request::new(&mut headers);
        if !matches!(request.parse(buf).ok()?, httparse::Status::Complete(_)) {
            return None;
        }
        extract_headers(request.headers)?
    };

    let raw_usn_str = raw.usn?;
    // Uppercase the serial to make the SsdpDevice::serial doc comment's "uppercase"
    // promise true. SSDP USN casing varies by firmware compile target, but MQTT broker
    // subscriptions and TLS SNI/identity route strictly on exact casing as printed on the
    // physical label — see reference/01_network_discovery.md §1.6 and
    // .claude/rules/tls-identity-sni.md.
    // A UPnP-compliant USN is compound: `uuid:<serial>::<urn>`. Truncating at the first `::`
    // keeps only the UUID part; without it the whole URN suffix is glued onto the serial, which
    // still passes resolve_model's 3-char prefix match and so is accepted as a corrupt serial —
    // one that then routes MQTT topics and TLS identity to a name the printer never answers to.
    let uuid_part = raw_usn_str.strip_prefix("uuid:").unwrap_or(raw_usn_str);
    let serial = uuid_part
        .split("::")
        .next()
        .unwrap_or(uuid_part)
        .to_ascii_uppercase();
    if serial.is_empty() {
        return None;
    }

    // Use DevModel header, falling back to model embedded in NT/ST per Protocol Violation #7.
    // A present-but-empty DevModel header (`Some("")`) must not short-circuit the
    // NT/ST fallback — `.filter()` treats it the same as absent, matching the intent of "use
    // the header if it actually carries a value."
    let mut effective_dev_model = raw.dev_model.filter(|s| !s.is_empty());
    let nt_st_model = raw.nt_or_st.and_then(extract_model_from_nt_st);

    let (ip, location_port) = raw.location.and_then(parse_location)?;
    let mut model = resolve_model(&serial, effective_dev_model);

    // Protocol Violation #7 requires the NT/ST fallback when `DevModel` is missing *or
    // malformed*, so it's conditioned on resolution failing, not on the header being absent: a
    // present-but-unrecognized token (a new SKU string, a firmware typo) would otherwise pin a
    // live printer to `PrinterModel::Unknown` and its conservative quirks profile even though
    // NT/ST names a model this crate knows. The retry is only adopted when it actually
    // resolves, so a junk `DevModel` is still reported verbatim in `raw_model_str` when NT/ST
    // can't do better.
    if model == PrinterModel::Unknown
        && let Some(nt_model) = nt_st_model
    {
        let retried = resolve_model(&serial, Some(nt_model));
        if retried != PrinterModel::Unknown {
            model = retried;
            effective_dev_model = Some(nt_model);
        }
    }

    // With no DevModel header at all, the NT/ST-embedded token is the only model string this
    // packet carries — report it in `raw_model_str` even when the serial prefix already
    // resolved the model without needing it.
    if effective_dev_model.is_none() {
        effective_dev_model = nt_st_model;
    }

    // Require a positive Bambu-specific signal before accepting the packet as a
    // printer record — USN+LOCATION alone is standard SSDP boilerplate any UPnP device
    // (routers, TVs, other vendors' printers) can supply. `model != Unknown` covers a
    // recognized serial prefix or `DevModel`; the NT/ST urn check also catches a genuine
    // Bambu device advertising only via that field with a serial prefix `resolve_model`
    // doesn't recognize (e.g. the generic `urn:bambulab-com:device:3dprinter:1` case).
    let is_bambu_device = model != PrinterModel::Unknown
        || raw
            .nt_or_st
            .is_some_and(|v| contains_ignore_ascii_case(v, "bambulab-com"));
    if !is_bambu_device {
        return None;
    }

    let signal_dbm = raw.dev_signal.and_then(|s| s.parse::<i32>().ok());

    Some(SsdpDevice {
        serial,
        model,
        name: raw.dev_name.unwrap_or("").to_owned(),
        ip,
        location_port,
        discovery_port: None,
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
        assert_eq!(
            parse_location("http://192.168.1.150:80/"),
            Some((ip("192.168.1.150"), 80))
        );
        assert_eq!(
            parse_location("https://10.0.0.42:8080/path"),
            Some((ip("10.0.0.42"), 8080))
        );

        // Regression (#328): the scheme is case-insensitive like the rest of this parser.
        assert_eq!(
            parse_location("HTTP://192.168.1.150:80/"),
            Some((ip("192.168.1.150"), 80))
        );
        assert_eq!(
            parse_location("Https://10.0.0.42:8080/"),
            Some((ip("10.0.0.42"), 8080))
        );
        assert_eq!(
            parse_location("http://[fe80::1]:80/"),
            Some((ip("fe80::1"), 80))
        );
    }

    #[test]
    fn test_parse_location_bare_ip() {
        assert_eq!(
            parse_location("192.168.1.158"),
            Some((ip("192.168.1.158"), 80))
        );
        assert_eq!(parse_location("[fe80::1]"), Some((ip("fe80::1"), 80)));
    }

    #[test]
    fn test_parse_location_rejects_non_ip_hosts() {
        assert_eq!(parse_location("http://printer.local:80/"), None);
        assert_eq!(parse_location("1.2.3.4@attacker.example.com"), None);
        assert_eq!(parse_location(""), None);
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn test_strip_bambu_suffix() {
        assert_eq!(strip_bambu_suffix("DevName.Bambu.COM"), "DevName");
        assert_eq!(strip_bambu_suffix("devname"), "devname");
        assert_eq!(strip_bambu_suffix(".bambu.com"), "");
        assert_eq!(strip_bambu_suffix("com"), "com");
    }

    #[test]
    fn test_contains_ignore_ascii_case() {
        assert!(contains_ignore_ascii_case(
            "urn:BambuLab-Com:device:3dprinter:1",
            "bambulab-com"
        ));
        assert!(!contains_ignore_ascii_case(
            "urn:other:device",
            "bambulab-com"
        ));
        assert!(!contains_ignore_ascii_case("short", "bambulab-com"));
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
        assert_eq!(device.model, PrinterModel::H2S);
        assert_eq!(device.ip.to_string(), "192.168.1.150");
        assert_eq!(device.location_port, 80);
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
        assert_eq!(device.model, PrinterModel::P1S);
        assert_eq!(device.ip.to_string(), "10.0.0.5");
        assert_eq!(device.location_port, 80);
    }

    #[test]
    fn test_parse_ssdp_lowercase_usn_serial_is_uppercased() {
        // Firmware-dependent USN casing must not leak into SsdpDevice::serial — the
        // doc comment promises "uppercase," and downstream MQTT subscription/TLS SNI routing
        // is exact-casing-sensitive (reference/01_network_discovery.md §1.6).
        let payload = b"HTTP/1.1 200 OK\r\n\
                        LOCATION: http://10.0.0.5:80/\r\n\
                        USN: 01p06a521703222\r\n\
                        DevModel.bambu.com: C12\r\n\r\n";

        let device = parse_ssdp_payload(payload).unwrap();
        assert_eq!(device.serial, "01P06A521703222");
        assert_eq!(device.model, PrinterModel::P1S);
    }

    #[test]
    fn test_parse_ssdp_compound_usn_yields_a_clean_serial() {
        // Regression (issue #104): a UPnP-compliant USN is `uuid:<id>::<urn>`. Keeping the URN
        // suffix produced a serial that still passed resolve_model's 3-char prefix match, so
        // the device was accepted with a corrupt serial that then drove MQTT topic routing and
        // TLS identity.
        let payload = b"HTTP/1.1 200 OK\r\n\
                        LOCATION: http://10.0.0.5:80/\r\n\
                        USN: uuid:01P06A521703222::urn:bambulab-com:device:3dprinter:1\r\n\
                        DevModel.bambu.com: C12\r\n\r\n";

        let device = parse_ssdp_payload(payload).unwrap();
        assert_eq!(device.serial, "01P06A521703222");
        assert_eq!(device.model, PrinterModel::P1S);
    }

    #[test]
    fn test_parse_ssdp_payload_leaves_discovery_port_unstamped() {
        // parse_ssdp_payload sees only datagram bytes, so it cannot know which socket the
        // packet arrived on, so it leaves SsdpDevice::discovery_port unknown.
        // DiscoveryEngine::poll_next_device is what stamps the real 2021/1990 value.
        let payload = b"HTTP/1.1 200 OK\r\n\
                        LOCATION: http://10.0.0.5:80/\r\n\
                        USN: 01P06A521703222\r\n\
                        DevModel.bambu.com: C12\r\n\r\n";

        assert_eq!(parse_ssdp_payload(payload).unwrap().discovery_port, None);
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
        assert_eq!(device.model, PrinterModel::P1S);
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
        assert_eq!(device.model, PrinterModel::P1S);
        assert_eq!(device.raw_model_str, "C12");
    }

    #[test]
    fn test_empty_dev_model_header_does_not_block_nt_st_fallback() {
        // A present-but-empty DevModel header (`Some("")`) previously short-circuited
        // the NT/ST fallback via `.or_else()`, which only triggers on `None`. Uses an
        // unrecognized serial prefix ("999") so resolve_model() must fall through to
        // effective_dev_model rather than resolving via the serial-prefix table directly.
        let payload = b"NOTIFY * HTTP/1.1\r\n\
                        HOST: 239.255.255.250:2021\r\n\
                        LOCATION: http://192.168.1.42:80/\r\n\
                        USN: 999123456789012\r\n\
                        DevModel.bambu.com: \r\n\
                        NT: urn:bambulab-com:device:C12:1\r\n\r\n";

        let device = parse_ssdp_payload(payload).unwrap();
        assert_eq!(device.model, PrinterModel::P1S);
        assert_eq!(device.raw_model_str, "C12");
    }

    #[test]
    fn test_malformed_dev_model_falls_back_to_nt_st() {
        // Protocol Violation #7's fallback covers a *malformed* DevModel, not just a missing
        // one: the header used to win unconditionally whenever it was non-empty, so an
        // unrecognized token plus a resolvable NT/ST resolved to Unknown and bound a live P1S
        // to the conservative fallback quirks profile. Unrecognized serial prefix ("999") so
        // the model can only come from the header or NT/ST.
        let payload = b"NOTIFY * HTTP/1.1\r\n\
                        HOST: 239.255.255.250:2021\r\n\
                        LOCATION: http://192.168.1.42:80/\r\n\
                        USN: 999123456789012\r\n\
                        DevModel.bambu.com: NOT-A-REAL-MODEL\r\n\
                        NT: urn:bambulab-com:device:C12:1\r\n\r\n";

        let device = parse_ssdp_payload(payload).unwrap();
        assert_eq!(device.model, PrinterModel::P1S);
        assert_eq!(device.raw_model_str, "C12");
    }

    #[test]
    fn test_unresolvable_dev_model_is_still_reported_verbatim() {
        // When NT/ST can't do better either, the junk DevModel token stays in raw_model_str
        // rather than being replaced by an equally unresolvable NT/ST token — the retry is
        // only adopted when it resolves.
        let payload = b"NOTIFY * HTTP/1.1\r\n\
                        HOST: 239.255.255.250:2021\r\n\
                        LOCATION: http://192.168.1.42:80/\r\n\
                        USN: 999123456789012\r\n\
                        DevModel.bambu.com: NOT-A-REAL-MODEL\r\n\
                        NT: urn:bambulab-com:device:3dprinter:1\r\n\r\n";

        let device = parse_ssdp_payload(payload).unwrap();
        assert_eq!(device.model, PrinterModel::Unknown);
        assert_eq!(device.raw_model_str, "NOT-A-REAL-MODEL");
    }

    #[test]
    fn test_nt_generic_3dprinter_not_used_as_model() {
        let payload = b"NOTIFY * HTTP/1.1\r\n\
                        HOST: 239.255.255.250:2021\r\n\
                        LOCATION: http://192.168.1.42:80/\r\n\
                        USN: 01P06A521703222\r\n\
                        NT: urn:bambulab-com:device:3dprinter:1\r\n\r\n";

        let device = parse_ssdp_payload(payload).unwrap();
        assert_eq!(device.model, PrinterModel::P1S);
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
    fn test_extract_model_from_nt_st_is_case_insensitive_like_is_bambu_device() {
        // `is_bambu_device` lowercases before matching the same URN, so a differently-cased
        // URN was accepted as a Bambu device while this fallback extracted nothing — the
        // printer came back `Unknown` with an empty `raw_model_str`.
        assert_eq!(
            extract_model_from_nt_st("URN:BambuLab-COM:device:P1S:1"),
            Some("P1S")
        );
        // Only the prefix comparison is normalized; the model keeps the wire's own casing.
        assert_eq!(
            extract_model_from_nt_st("URN:BAMBULAB-COM:DEVICE:p1s:1"),
            Some("p1s")
        );
        // The generic-model rejection was already case-insensitive and must stay that way.
        assert_eq!(
            extract_model_from_nt_st("URN:BambuLab-COM:device:3DPrinter:1"),
            None
        );
        // A URN shorter than the prefix must not panic on the split.
        assert_eq!(extract_model_from_nt_st("urn:bambu"), None);
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
        assert_eq!(device.model, PrinterModel::P1S);
        assert_eq!(device.ip.to_string(), "192.168.1.158");
        assert_eq!(device.location_port, 80);
        assert_eq!(device.name, "3DP-01P-981");
        assert_eq!(device.signal_dbm, Some(-43));
        assert_eq!(device.bind_state, "free");
        assert_eq!(device.security_link, "secure");
        assert_eq!(device.version, "01.10.00.00");
        assert_eq!(device.connect_type, "lan");
    }

    #[test]
    fn test_non_bambu_device_rejected() {
        // Ordinary UPnP devices (routers, TVs, other vendors' printers) can supply a
        // USN+LOCATION SSDP packet with no Bambu-specific header at all — must not be accepted
        // as a printer record.
        let payload = b"NOTIFY * HTTP/1.1\r\n\
                        HOST: 239.255.255.250:1900\r\n\
                        LOCATION: http://192.168.1.99:80/description.xml\r\n\
                        USN: uuid:12345678-1234-1234-1234-123456789012\r\n\
                        NT: urn:schemas-upnp-org:device:MediaRenderer:1\r\n\r\n";

        assert!(parse_ssdp_payload(payload).is_none());
    }

    #[test]
    fn test_lowercase_status_line_routes_to_response_parser() {
        // is_response's classification is case-insensitive, matching this
        // file's otherwise-thorough case-insensitive handling elsewhere. Note this only
        // fixes *classification* — httparse::Response::parse itself requires an exact-case
        // "HTTP/" token in the status line and rejects "Http/1.1" regardless of which
        // parser it's routed to, so the packet is still correctly rejected end-to-end
        // (None), just no longer via the wrong parser. Real firmware has never been
        // observed emitting non-canonical case, so this is a defense-in-depth correctness
        // fix, not a behavior change for real traffic.
        let payload = b"Http/1.1 200 OK\r\n\
                        LOCATION: http://10.0.0.5:80/\r\n\
                        USN: 01P06A521703222\r\n\
                        DevModel.bambu.com: C12\r\n\r\n";

        assert!(parse_ssdp_payload(payload).is_none());
    }

    #[test]
    fn test_truncated_packet_rejected() {
        // httparse::Status::Partial (buffer ends mid-header) must be rejected,
        // not treated the same as Status::Complete — a truncated UDP datagram shouldn't
        // parse into a seemingly-valid device record.
        let payload = b"HTTP/1.1 200 OK\r\n\
                        LOCATION: http://10.0.0.5:80/\r\n\
                        USN: 01P06A521703222\r\n\
                        DevModel.bambu.com: C12\r\n";

        assert!(parse_ssdp_payload(payload).is_none());
    }

    #[test]
    fn test_unparseable_port_rejected() {
        // A present-but-unparseable port string must reject the packet, not
        // silently coerce to 80 — that's indistinguishable from "no port specified."
        assert_eq!(parse_location("192.168.1.158:notaport"), None);
        // Absent port still defaults to 80.
        assert_eq!(
            parse_location("192.168.1.158"),
            Some((ip("192.168.1.158"), 80))
        );
    }
}
