# Chapter 1: Network Transport Layer & Device Discovery

---

### 1.1 Simple Service Discovery Protocol (SSDP) [REF-NET-DISC]

Physical Bambu Lab printers utilize a modified, local-network implementation of the Simple Service Discovery Protocol (SSDP) to advertise their presence and respond to active client discovery queries on the local network.

#### Multicast Configuration
*   **Multicast IPv4 Address**: `239.255.255.250`
*   **SSDP Ports**: `2021` and `1990` (Note: Both deviate from the UPnP standard SSDP Port `1900`. The official Bambu Lab network port documentation lists both ports for device discovery. Port behavior varies by model: the P1S (firmware 01.10.00.00) responds to M-SEARCH on port 1990 but only sends passive NOTIFY on port 2021. Newer models likely use port 2021 for active M-SEARCH responses. Discovery clients should bind both ports.)
*   **Search Target (ST)**: `urn:bambulab-com:device:3dprinter:1`

#### Client Socket Bind Requirement
Discovery clients **must** bind their UDP socket to the SSDP port (e.g., `0.0.0.0:2021`) rather than an ephemeral port (`0.0.0.0:0`). Printers send NOTIFY advertisements to the multicast group address on the SSDP port, and the operating system only delivers multicast packets to sockets whose bound port matches the destination port of the incoming datagram. Binding to an ephemeral port will receive unicast M-SEARCH responses (if the printer sends them) but will silently miss all multicast NOTIFY traffic. Clients targeting both ports should bind separate sockets to `2021` and `1990`.

#### Active Discovery Query (M-SEARCH)
To discover printers on the local network, the client must transmit a unicast or multicast UDP datagram to Port `2021` (and/or `1990`). The payload must conform to the following HTTP/1.1-style layout and must be strictly terminated with a double CRLF (`\r\n\r\n`) sequence for the printer's lightweight embedded firmware parser to process it.

```http
M-SEARCH * HTTP/1.1
HOST: 239.255.255.250:2021
MAN: "ssdp:discover"
MX: 3
ST: urn:bambulab-com:device:3dprinter:1


```

#### Unicast Discovery Header Constraint
When targeting a physical printer with a unicast query, the printer's SSDP parser accepts either the multicast group address `HOST: 239.255.255.250:2021` or the printer's own IP address `HOST: <printer_ip>:2021` in the `HOST` header.

#### Passive Advertisement (NOTIFY)
The physical printer periodically broadcasts multicast UDP datagrams to `239.255.255.250` on the SSDP port to advertise its active operational status on the local subnet. The NOTIFY payload format varies significantly between firmware generations and printer models. Discovery parsers must treat all non-`USN` and non-`LOCATION` fields as optional.

**Composite example (headers observed across multiple models and firmware tracks):**

```http
NOTIFY * HTTP/1.1
HOST: 239.255.255.250:2021
Server: UPnP/1.0
CACHE-CONTROL: max-age=1800
LOCATION: http://192.168.1.150:80/
NT: urn:bambulab-com:device:3dprinter:1
NTS: ssdp:alive
USN: 09406A521703533
DevName.bambu.com: MyPrinterName
DevModel.bambu.com: N7
DevSignal.bambu.com: -43
DevConnect.bambu.com: lan
DevBind.bambu.com: bound
DevSeclink.bambu.com: secure
DevInf.bambu.com: wlan0
DevVersion.bambu.com: 01.02.00.00
DevCap.bambu.com: 1
```

#### Verified P1S NOTIFY (firmware 01.10.00.00)
The following packet was captured from a physical P1S (serial prefix `01P`, DevModel `C12`) running firmware `01.10.00.00` in LAN/Developer mode. It deviates from the composite example above in several ways documented in Protocol Violations below.

```http
NOTIFY * HTTP/1.1
HOST: 239.255.255.250:1900
Server: UPnP/1.0
Location: 192.168.1.158
NT: urn:bambulab-com:device:3dprinter:1
USN: 01P00A4C2009981
Cache-Control: max-age=1800
DevModel.bambu.com: C12
DevName.bambu.com: 3DP-01P-981
DevSignal.bambu.com: -43
DevConnect.bambu.com: lan
DevBind.bambu.com: free
Devseclink.bambu.com: secure
DevVersion.bambu.com: 01.10.00.00
DevCap.bambu.com: 1
```

#### NOTIFY Advertisement Interval
The P1S (firmware 01.10.00.00) sends NOTIFY advertisements at a consistent interval of approximately **10.1 seconds** (measured over 7 consecutive packets). The `Cache-Control: max-age=1800` header indicates a 30-minute validity window. Discovery clients should allow at least 20 seconds of listening time to guarantee capturing one full NOTIFY cycle, accounting for multicast group join latency and clock jitter.

#### Unicast Search Response (M-SEARCH Reply)
When a valid `M-SEARCH` query is received on the SSDP port, some printer models emit a unicast UDP response directly back to the sender's source port. **Port behavior varies by model and firmware generation.** The P1S (firmware 01.10.00.00) responds to M-SEARCH queries on port 1990 (within ~5 seconds) but ignores M-SEARCH on port 2021, where it relies entirely on periodic NOTIFY advertisements at ~10.1-second intervals. Newer models likely respond on port 2021. Discovery engines should query both ports and also listen for NOTIFY traffic as a fallback.

```http
HTTP/1.1 200 OK
CACHE-CONTROL: max-age=1800
LOCATION: http://192.168.1.150:80/
ST: urn:bambulab-com:device:3dprinter:1
USN: 09406A521703533
DevName.bambu.com: MyPrinterName
DevModel.bambu.com: N7
DevConnect.bambu.com: lan
DevBind.bambu.com: bound
DevSeclink.bambu.com: secure
DevInf.bambu.com: wlan0
DevVersion.bambu.com: 01.02.00.00
DevCap.bambu.com: 1
```

#### Protocol Violations & UPnP Deviations
1.  **Bare `USN` Format**: UPnP architecture specifies that the `USN` (Unique Service Name) header must be a URI prefixed with `uuid:`. The printer's firmware frequently violates this constraint, transmitting only the bare, uppercase hardware serial number (e.g., `USN: 09406A521703533`). Discovery parsers must process both the bare serial and `uuid:` prefixed variants to maintain cross-generation compatibility.
2.  **Inactive `LOCATION` Port**: SSDP response payloads contain a `LOCATION` header pointing to Port `80` (e.g., `LOCATION: http://<ip_address>:80/`). The physical printer does not run an HTTP server on Port `80` on modern firmware tracks; incoming connections to Port `80` are refused.
3.  **Bare `LOCATION` Format**: Some firmware tracks (confirmed on P1S 01.10.00.00) transmit the `Location` header as a bare IP address (e.g., `Location: 192.168.1.158`) without the `http://` scheme prefix or port suffix. Discovery parsers must handle both the full URI format (`http://<ip>:80/`) and bare IP formats.
4.  **`HOST` Header Port Mismatch**: The P1S sends NOTIFY packets to the multicast group on Port `2021` (the actual UDP destination), but the `HOST` header within the payload body contains `239.255.255.250:1900` (referencing the standard UPnP port). Discovery engines should rely on the socket's bound port for reception, not the `HOST` header value.
5.  **Header Casing Inconsistency**: Header names may appear with varying capitalization across firmware tracks (e.g., `DevSeclink.bambu.com` vs `Devseclink.bambu.com`, `LOCATION` vs `Location`, `CACHE-CONTROL` vs `Cache-Control`). Discovery parsers must perform case-insensitive header matching.
6.  **Optional Headers**: The `NTS`, `DevInf.bambu.com`, and `Server` headers are not present on all firmware tracks. The `DevSignal.bambu.com` header (WiFi RSSI in dBm) is present on some models but undocumented in older references. Parsers must treat all headers except `USN` and `LOCATION` as optional.
7.  **Dynamic Notification Target (`NT` / `ST`)**: Depending on the active firmware track, the notification target (`NT`) and search target (`ST`) headers may dynamically include the actual printer model (e.g., `urn:bambulab-com:device:P1S:1`) instead of the generic `3dprinter:1`. Discovery engines must fall back to searching the target string for direct model extraction if `DevModel.bambu.com` is missing or malformed.

---

### 1.2 Direct Local Port Mapping Matrix [REF-NET-PORTS]

The physical printer exposes a dedicated set of network interfaces on the local subnet to coordinate telemetry, file transfers, secure video streams, and low-power frame-buffer extraction.

| Physical Port | Protocol | Layer Type | Functional Purpose | Model Availability |
| :--- | :--- | :--- | :--- | :--- |
| **2021 / 1990** | UDP | Multicast/Unicast | SSDP Discovery Broadcaster and Query Responder | All Models |
| **8883** | TCP | TLS (MQTTS) | Local Broker Control & State Telemetry Stream | All Models |
| **990** | TCP | Implicit TLS (FTPS) Only | MicroSD Storage Traversal, 3MF Print Transfer, Logs | All Models |
| **322** | TCP | TLS (RTSPS) | H.264 video stream extraction via RTSPS over TLS (disabled by default on H2 series*) | X1, X1C, X1E, X2D, P2S, H2C, H2D, H2D Pro, H2S |
| **6000** | TCP | TLS Socket | Direct chunked JPEG frame-buffer extraction | A1, A1 Mini, A2L, P1P, P1S |

\*Note: For the H2 series (H2S, H2D, H2C, H2D Pro), Port 322 is closed by default (`ECONNREFUSED`) in factory firmware. Telemetry reports `"rtsp_url": "disable"` until manually enabled via the physical touchscreen interface.

\*\*Note: A2L added to the port 6000 model list above — this table originally predated the A2L's release. No independent wire capture exists for this printer yet; the binary-JPEG classification is inferred from OrcaSlicer's A2L machine profile, which groups its camera handling with A1/A1 Mini/P1P/P1S rather than the RTSPS-series models. Treat as unconfirmed until verified against real A2L traffic.

---

### 1.3 Cryptographic Context & Handshakes [REF-NET-SECURE]

All communication channels (Ports 8883, 990, 322, 6000) are wrapped in secure TLS contexts. Due to the embedded constraints of the physical server, the connection negotiations must adhere to specific handshake and trust properties.

#### Trust Verification & Hostname Override (SNI)
The printer's self-signed X.509 certificates contain the printer's unique uppercase serial number in the Common Name (CN) field. During TLS handshakes, the printer expects the Server Name Indication (SNI) extension to match this serial number if hostname verification is enforced by the connecting peer.

#### X.509 Certificate Key Usage Constraints
The self-signed certificates generated by the physical printer's embedded server do not contain X.509 v3 "Key Usage" extension blocks.

#### Trust Anchors: The BBL CA Bundle [REF-NET-CABUNDLE]

Printer leaf certificates chain to a private Bambu Lab CA, never to a public root, so a peer enforcing verification must supply BBL's anchors explicitly. Bambu ships them as a five-certificate PEM bundle in BambuStudio at `resources/cert/printer.cer`; ha-bambulab mirrors that file verbatim, and its `fetch_certs_github.py` pulls the canonical copy from:

`https://raw.githubusercontent.com/bambulab/BambuStudio/refs/heads/master/resources/cert/printer.cer`

Bambu is mid-rollover between two CA generations, and **the order of certificates within the file is load-bearing**:

| # | Subject | Issuer | Validity |
|---|---|---|---|
| 1 | `CN=BBL CA2 RSA` | self | 2025-06-17 → 2050-06-17 |
| 2 | `CN=BBL CA2 ECC` | self | 2025-06-17 → 2050-06-17 |
| 3 | `CN=BBL CA2 RSA` | `CN=BBL CA` | 2025-06-17 → 2035-06-15 |
| 4 | `CN=BBL CA2 ECC` | `CN=BBL CA` | 2025-06-17 → 2035-06-15 |
| 5 | `CN=BBL CA` | self | 2022-04-04 → 2032-04-01 |

Entries 3 and 4 are cross-signs, letting a client that only trusts the legacy root reach the new sub-CAs. A P1S chains directly to the **legacy `CN=BBL CA` root, which is last in the file** — so a client that parses only the first certificate of the bundle loads `BBL CA2 RSA` and cannot verify a P1S at all. Newer models chain leaf → `BBL Device CA <model>-V2` → `BBL CA2 RSA`. Covering the model range therefore requires trusting several anchors at once; picking one means not supporting some printer.

Two consequences for any implementation:

*   **Concatenated DER does not work where the parser sniffs format.** mbedTLS selects its parse strategy from the buffer: `mbedtls_x509_crt_parse` takes the multi-certificate PEM loop only when the buffer is NUL-terminated *and* contains a `BEGIN CERTIFICATE` header, and otherwise calls `mbedtls_x509_crt_parse_der`, which parses exactly one certificate and returns. A NUL-terminated PEM bundle is the only encoding that carries more than one anchor through a single buffer.
*   **Partial parse failure is tolerated, not fatal.** ESP-IDF's `set_ca_cert` (`components/esp-tls/esp_tls_mbedtls.c`) continues when `mbedtls_x509_crt_parse` reports a positive count of failed certificates, logging `mbedtls_x509_crt_parse was partly successful. No. of failed certificates: %d` at warning level. A handshake can therefore succeed on a partially-loaded trust store; the absence of that warning is the signal that every anchor loaded.

Verified on hardware (ESP32-C6 against a P1S, 2026-08-23): all five certificates parse and load, no partial-failure warning is emitted, and withholding entry 5 makes the handshake fail with `MBEDTLS_ERR_X509_CERT_VERIFY_FAILED` (`-0x2700`) while entry 5 alone succeeds. Chain validation against the `BBL CA2` anchors is **unverified** — it needs a model that actually chains to them (X1 or P2 series). Note also that the anchor set is only one of three things that must line up: the printer must present a chain reaching a trusted anchor, and its leaf must satisfy the name check (see *Trust Verification & Hostname Override* above), both of which are per-model wire facts.

Because this bundle is vendored from an upstream that rotates it (CA2 landed in June 2025), treat a change to `resources/cert/printer.cer` as a signal to re-check, not a one-time capture.

#### Handshake Latency & RTOS Process Limits
ESP32-based RTOS hardware lines (P1 and A1 series) exhibit significant cryptographic latency during secure MQTTS (Port 8883) handshakes, requiring up to 5.0 seconds to return an MQTT `CONNACK` on a cold session.

#### Where the handshake time actually goes

Measured on an ESP32-C6 client against a P1S over Wi-Fi, using TLS 1.2 and the full five-anchor bundle. A typical handshake took about 1.4 seconds, split like this:

| Part | Time | Notes |
|---|---|---|
| Waiting for the printer to reply | ~800 ms | The printer is simply slow. A laptop on the same network, with a 4.7 ms round trip, also needs 805 ms (±2%), so this is not specific to small clients. |
| Client-side crypto | ~400 ms | Key exchange plus certificate checks. |
| Setting up TLS and parsing the five trust anchors | ~26 ms | Happens once at the start of each handshake. |
| Waiting between poll attempts | ~0 ms | Not a real cost — see below. |

Three things that look like they should help, but don't:

*   **Polling faster.** Making the client check four times as often changed total polling time by 0.4%. Each poll is already waiting on a printer that has not answered yet.
*   **Turning off Nagle (`TCP_NODELAY`).** One round appeared to save ~97 ms, but a repeat run with identical code gave it all back. Two runs of the same build differ by more than that, so there is no measurable effect. It is still set, because it is standard practice for small-message protocols, not because it was shown to help.
*   **Shipping fewer trust anchors.** Parsing all five costs 26 ms, so this could never save more than that — and a partial bundle verifies some printer models while failing others, which is much worse than 26 ms. See [REF-NET-CABUNDLE].

**TLS session resumption does not work with these printers.** The printer offers a 32-byte session ID but never issues a session ticket, and it refuses to resume when the client offers that exact ID back. This was confirmed by reading the handshake messages, not guessed from timing: the client sent the printer's own session ID byte for byte, and the printer started a brand new session anyway. So each connection pays the full cost. This matters more than it first appears, because MQTT, FTPS, and the camera are three separate connections.

#### Choosing an elliptic curve that the ESP32 can accelerate

By default mbedTLS asks for Curve25519 first, and Bambu printers accept it. But the ESP32's crypto accelerator only handles the P-192 and P-256 curves, so Curve25519 runs in software and the hardware sits idle.

Building with `CONFIG_MBEDTLS_ECP_DP_CURVE25519_ENABLED=n` leaves P-256 at the front of the list instead. Measured effect on the same ESP32-C6 and P1S, changing nothing else:

| | Curve25519 | P-256 |
|---|---|---|
| Client-side crypto | 409.5 ms | **179.9 ms** |
| Slowest single step | 260–316 ms | 56–57 ms |
| Whole handshake | 1318 ms | **1136 ms** |

That is about 230 ms of crypto work saved per handshake, and it was reproduced in two separate runs. Verified against all three of the printer's TLS services — MQTT (8883), FTPS (990), and the camera (6000) — all of which connected normally with Curve25519 turned off.

Two caveats before relying on this:

1.  **Only a P1S has been checked.** Other models run different firmware, and this chapter and [REF-FTPS-CONN] already document TLS differences between models (P2S and X2D both need TLS 1.2 for unrelated reasons). Test your model before assuming it works.
2.  **This setting applies to the whole firmware image, not just printer connections.** If the same device also talks to a server that requires Curve25519, that connection will stop working. It is a compile-time option, so it cannot be turned on and off per connection.

#### Authentication Credentials
Local MQTTS and FTPS sessions utilize a unified credential pair:
*   **Username**: `bblp`
*   **Password**: The 8-character, uppercase alphanumeric access code printed on the machine's physical LCD screen.

---

### 1.4 SSDP Hardware Model Code Mapping

Discovery systems parse the incoming `DevModel.bambu.com` (or `DevModel`) header to identify the physical capabilities of the machine and route control commands to the correct physical ports.

| SSDP DevModel Value | Display Name | Core Architecture Family | Camera Protocol Target |
| :--- | :--- | :--- | :--- |
| **`BL-P001`** | X1 / X1C | CoreXY | RTSPS (Port 322) |
| **`C13`** | X1E | CoreXY | RTSPS (Port 322) |
| **`N6`** | X2D | CoreXY | RTSPS (Port 322) |
| **`N1`** | A1 Mini | Bed Slinger | Binary JPEG Stream (Port 6000) |
| **`N2S`** | A1 | Bed Slinger | Binary JPEG Stream (Port 6000) |
| **`N9`** | A2L | Bed Slinger | Binary JPEG Stream (Port 6000) |
| **`C11`** | P1P | CoreXY | Binary JPEG Stream (Port 6000) |
| **`C12`** | P1S | CoreXY | Binary JPEG Stream (Port 6000) |
| **`N7`** | P2S | CoreXY | RTSPS (Port 322) |
| **`O1D`** | H2D | CoreXY | RTSPS (Port 322) |
| **`O1E` / `O2D`** | H2D Pro | CoreXY | RTSPS (Port 322) |
| **`O1C`** | H2C | CoreXY | RTSPS (Port 322) |
| **`O1C2`** | H2C (Dual Nozzle variant) | CoreXY | RTSPS (Port 322) |
| **`O1S`** | H2S | CoreXY | RTSPS (Port 322) |

---

### 1.5 Printer Serial Prefix Mapping Table

Discovery systems may match the leading 3-character prefix of the printer serial number provided by the parsed `USN` header string to determine the printer model. Subsequent characters of the printer serial number represent revision, manufacturing date, and hardware variation codes which are not part of the model identifier.

| Prefix | Printer Model | Printer Series |
| :--- | :--- | :--- |
| **`094`** | H2D | H2 |
| **`093`** | H2S | H2 |
| **`239`** | H2D Pro | H2 |
| **`31B`** | H2C | H2 |
| **`00M`** | X1C / X1 | X1 |
| **`03W`** | X1E | X1 |
| **`20P`** | X2D | X2 |
| **`01S`** | P1P | P1 |
| **`01P`** | P1S | P1 |
| **`22E`** | P2S | P2 |
| **`030`** | A1 Mini | A1 |
| **`039`** | A1 | A1 |
| **`26A`** | A2L | A2 |

---

### 1.6 Mechanical & Firmware Discovery Quirks

#### SSDP Unicast Query Availability
The physical printer processes direct unicast UDP queries on its active SSDP port. If multicast routing is restricted on the local subnet, the printer may still process and respond to unicast `M-SEARCH` queries, though this behavior is not reliable across all models (see M-SEARCH Reply section above).

#### Post-Boot Local Broker Handshake Delay
On A1 and P1 firmware tracks, the local MQTT broker socket (Port 8883) is temporarily unavailable and rejects incoming connection attempts for approximately 15 to 30 seconds after hardware boot while the system completes its self-test loops.

#### Case-Sensitive Serial Routing
All MQTTS topic structures are case-sensitive. While the SSDP `USN` field may return serial numbers with mixed casing depending on the firmware compile target, the local MQTT broker strictly routes commands to the topic using the exact casing printed on the printer's physical mainboard label. Incorrect casing results in an accepted subscription but zero emitted telemetry packets (`[REF-DIAG-HMS]` warning condition).
