//! Current investigation: GitHub issue #160, round 5 — does TLS session resumption work
//! against a Bambu printer, and what does it save?
//!
//! Rounds 1-4 accounted for the whole ~1.4s handshake on an ESP32-C6 against a P1S:
//! ~800ms waiting on the printer, ~400ms local mbedTLS compute, ~26ms SSL setup and
//! trust-store parse, and ~0ms of poll loop. `TLS_POLL_INTERVAL` and Nagle were both ruled
//! out as costs, and trimming the anchor bundle was ruled out as a saving worth its
//! reliability price. The peer figure is not inference: the same handshake from a laptop
//! on the same LAN takes 805ms +/- 2%, so the printer is slow for every client.
//!
//! That leaves resumption as the only remaining lever, and the only one that touches the
//! *dominant* term. An abbreviated handshake skips the ECDHE key exchange and the
//! certificate chain verification — which is most of our ~400ms **and** most of what the
//! printer spends its ~800ms on. It also matters more than one handshake's worth: MQTT,
//! FTPS and the camera each dial independently (`ensure_mqtt`/`ensure_ftps`/`ensure_camera`
//! in `src/client/connect.rs`), so a consumer using all three currently pays the full cost
//! three times. See GitHub issue #161 for overlapping those dials, which is complementary.
//!
//! **Why this probe bypasses `bambino`.** Every other probe in this harness drives the
//! shipped type on purpose. This one cannot yet: `esp-idf-svc`'s safe `Config`
//! (`tls.rs:147`) exposes 16 fields and `client_session` is not among them, so resumption
//! is unreachable through `EspTls::negotiate`. It *is* reachable one layer down —
//! `EspTls::adopt` is three `sys::` calls and `negotiate` is an `esp_tls_conn_new_sync` plus
//! a return-code match, all public symbols this crate already uses elsewhere. So the plan
//! is: prove it works here against the real printer first, and only then own the
//! `esp_tls_cfg` inside `EspIdfTlsConnector::connect`. Building that into the crate before
//! knowing whether the printer issues resumable sessions would be speculative surgery on
//! the one function GitHub issues #61, #67 and #156 all landed in.
//!
//! **Requires `CONFIG_ESP_TLS_CLIENT_SESSION_TICKETS=y`** (set in `sdkconfig.defaults`).
//! It is off by ESP-IDF default, and without it `esp_tls_get_client_session` and
//! `esp_tls_cfg_t.client_session` are not generated into `bindings.rs` at all — the mbedTLS
//! half, `CONFIG_MBEDTLS_CLIENT_SSL_SESSION_TICKETS`, is already on without asking.
//!
//! **Single DER anchor, deliberately.** `esp_tls_cfg_t` takes a chain only in PEM form; in
//! DER it accepts exactly one certificate. This probe passes `bbl_5.der` (`CN=BBL CA`), the
//! anchor a P1S chains to — confirmed by issue #157's probe, where withholding precisely
//! that anchor produced `UntrustedAnchor`. That is fine for a spike measuring resumption,
//! and it is **not** a model for shipped configuration: issue #145 is the failure where a
//! partial trust store verifies some models, fails others, and looks identical to a clean
//! handshake in the log. The real connector keeps all five anchors.
//!
//! ## What the run tells you
//!
//! Phase A runs [`FULL_RUNS`] full handshakes and, after each, exports the session and reads
//! its `id_len` and `ticket_len`.
//!
//! - **Both zero** -> the printer negotiated a session but supplied nothing to resume with.
//!   Resumption is dead for this model, #160 closes as irreducible, and no crate change is
//!   worth making. Note that `esp_tls_get_client_session()` returns non-NULL in this case
//!   anyway, so the pointer alone proves nothing (see `handshake()`).
//! - **Either non-zero** -> phase B replays it.
//!
//! Phase B runs [`RESUMED_RUNS`] handshakes with the captured session installed, taking a
//! fresh session after each in case the printer treats them as single-use.
//!
//! - **Phase B markedly faster** (expect ~150-250ms against ~1400ms if the abbreviated
//!   handshake behaves as the RFC describes) -> build it into the crate.
//! - **Phase B the same as phase A** -> the peer declined to resume what it advertised. An
//!   abbreviated handshake is one round trip with no ECDHE and no chain verification, so
//!   full-handshake time in phase B settles that on duration alone, without a capture.
//!
//! ## Round 5's answer: resumption is unavailable against a P1S
//!
//! **Attempt 1** failed in phase B in 28-35ms every time with a bare -1 — a setup error, not
//! a rejected handshake, and ours rather than the printer's. esp-tls calls
//! `mbedtls_ssl_set_session()` on *every* entry to `esp_mbedtls_handshake`
//! (`esp_tls_mbedtls.c:281`), while mbedTLS rejects the second call once `handshake->resume`
//! is set (`ssl_tls.c:1546`, `MBEDTLS_ERR_SSL_FEATURE_UNAVAILABLE`). That is invisible in
//! esp-tls's normal blocking use, where one call completes the whole handshake, and only
//! appears for a caller that takes one step per call — which is what `timeout_ms = 0`
//! (issue #67) makes this crate do. `handshake()` clears `cfg.client_session` after the
//! first call. **Any implementation inside `EspIdfTlsConnector::connect` must do the same,
//! and this is the concrete reason the cfg has to be owned rather than borrowed from
//! `esp-idf-svc`.** `esp_tls.c`'s `case ESP_TLS_CONNECTING` falls through into
//! `case ESP_TLS_HANDSHAKE` with no `break`, so the first call really does reach the
//! handshake and the session really is offered once.
//!
//! **Attempt 2** ran clean and saved nothing: 1331ms full vs 1263ms resumed. The session
//! fields say why. The printer supplies `id_len 32, ticket_len 0` — a session ID, never a
//! ticket — and a phase B handshake still costs full-handshake time, which an abbreviated
//! one cannot. **The P1S advertises a session ID and then does not honour it**, i.e. it
//! keeps no server-side session cache. There is nothing a client can do about that.
//!
//! A caution for anyone re-running this: `esp_tls_get_client_session()` returning non-NULL
//! was misread once as "the printer supports resumption". It does not mean that. On TLS 1.2
//! it is a bare `mbedtls_ssl_get_session()` and succeeds for any negotiated session. The
//! `id_len`/`ticket_len` readout exists because of that mistake — trust it, not the pointer.
//!
//! Remember the measurement floor established in rounds 3 and 4: against a peer that swings
//! 820-1644ms, only effects larger than ~200ms are demonstrable at this sample size. Two
//! runs of identical code differed by 141ms. Resumption should clear that bar by a wide
//! margin; if the difference is marginal, treat it as noise rather than a small win.
//!
//! **Setup.** Certificates are not committed (see `.gitignore`) — regenerate with:
//!
//! ```sh
//! cd esp32-hw-probe && mkdir -p certs && cd certs \
//!   && awk '/-----BEGIN CERTIFICATE-----/{n++} n{print > ("bbl_" n ".pem")}' \
//!        ../../../BambuStudio/resources/cert/printer.cer \
//!   && for n in 1 2 3 4 5; do openssl x509 -in bbl_$n.pem -outform DER -out bbl_$n.der; done \
//!   && rm -f bbl_*.pem
//! ```
//!
//! Network and printer details come from a gitignored `esp32-hw-probe/.env`, read by
//! `build.rs` and compiled in via `env!(..)`, so no Wi-Fi password, printer IP, or serial is
//! written into a tracked file or typed on a command line where it would land in shell
//! history. Copy `.env.example` to `.env` and fill it in. (Root `CLAUDE.md` treats serials as
//! credentials.) No access code is needed: the TLS handshake completes before MQTT
//! authentication, which is all this probe reaches.
//!
//! ```sh
//! cd esp32-hw-probe && cargo espflash flash --release --monitor 2>&1 | tee run.log
//! ```
//!
//! Prior investigations (issue #160 rounds 1-4's handshake cost breakdown, issue #157's
//! certificate-failure probe, issue #145's multi-anchor bundle probe, issue #65's
//! concurrent-sleep probe) are recoverable via `git log -- esp32-hw-probe/src/main.rs`.

use core::time::Duration;
use std::ffi::CString;
use std::os::fd::{AsRawFd, IntoRawFd};
use std::time::Instant;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sys;
use esp_idf_svc::tls::{EspTls, Socket};
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};

/// `CN=BBL CA`, the self-signed anchor a P1S chains to. See this file's header for why one
/// DER anchor rather than the full five-anchor PEM bundle the shipped connector uses.
const BBL_CA_ANCHOR: &[u8] = include_bytes!("../certs/bbl_5.der");

const WIFI_SSID: &str = env!("PROBE_WIFI_SSID");
const WIFI_PASS: &str = env!("PROBE_WIFI_PASS");
const PRINTER_IP: &str = env!("PROBE_PRINTER_IP");
/// Passed as the TLS hostname, mirroring `src/client/connect.rs`. The printer's leaf is
/// `CN=<serial>` with no SAN, so verifying against the dialled IP would fail the common-name
/// check for reasons that have nothing to do with resumption.
const PRINTER_SERIAL: &str = env!("PROBE_SERIAL");

const PRINTER_TLS_PORT: u16 = 8883;

/// Matches `TLS_POLL_INTERVAL` in `src/io/esp_idf.rs`, so phase A's timings stay comparable
/// with rounds 1-4. Round 2 established that this value is not itself a cost.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Matches the crate's `timeout_ms = 0` (issue #67): one handshake step per call, returning
/// immediately, so the loop below paces rather than blocking inside the FFI call.
const STEP_TIMEOUT_MS: i32 = 0;

/// Upper bound on one handshake, mirroring the crate's default connect timeout.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);

/// Settle time between handshakes, so a lingering half-open connection cannot perturb the
/// next run's timing.
const BETWEEN_RUNS: Duration = Duration::from_secs(3);

// Two apiece rather than four: mbedTLS debug level 3 prints roughly a screen per handshake,
// and this round is answering a yes/no question about the protocol rather than measuring a
// distribution. Put them back up if you turn the debug options in `sdkconfig.defaults` off.
const FULL_RUNS: usize = 2;
const RESUMED_RUNS: usize = 2;

/// Adapts a `std::net::TcpStream` to `esp-idf-svc`'s `Socket` so `EspTls::adopt` will take it.
///
/// `release` must hand the fd over without closing it: ESP-IDF closes the socket itself when
/// the TLS context is destroyed, and a double close would land on whatever fd the allocator
/// handed out next.
struct ProbeSocket(Option<std::net::TcpStream>);

impl Socket for ProbeSocket {
    fn handle(&self) -> i32 {
        self.0.as_ref().map(|s| s.as_raw_fd()).unwrap_or(-1)
    }

    fn release(&mut self) -> Result<(), sys::EspError> {
        if let Some(stream) = self.0.take() {
            let _ = stream.into_raw_fd();
        }
        Ok(())
    }
}

/// One handshake's outcome.
struct Outcome {
    elapsed_ms: u128,
    /// The resumable session the printer left behind, if it left one.
    session: Option<*mut sys::esp_tls_client_session_t>,
}

fn main() {
    sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    // `timeout_ms = 0` makes esp-tls log a spurious warning on every handshake step that
    // does not complete — ~50 per handshake, all claiming a failure that never happened.
    // The crate handles this with `EspTlsLogQuiet` (issue #156); that type is private, so
    // the probe does the same thing directly. ERROR rather than NONE so a real failure
    // still reaches the transcript.
    unsafe {
        let tag = CString::new("esp-tls").expect("static tag has no interior NUL");
        sys::esp_log_level_set(tag.as_ptr(), sys::esp_log_level_t_ESP_LOG_ERROR);
    }

    log::info!("esp32-hw-probe: issue #160 round 5, TLS session resumption");
    log::info!("target {PRINTER_IP}:{PRINTER_TLS_PORT}, single DER anchor (CN=BBL CA)");

    let peripherals = match Peripherals::take() {
        Ok(p) => p,
        Err(e) => {
            log::error!("FAIL setup: Peripherals::take() failed: {e:?}");
            park();
        }
    };
    let sysloop = match EspSystemEventLoop::take() {
        Ok(s) => s,
        Err(e) => {
            log::error!("FAIL setup: EspSystemEventLoop::take() failed: {e:?}");
            park();
        }
    };
    let nvs = match EspDefaultNvsPartition::take() {
        Ok(n) => n,
        Err(e) => {
            log::error!("FAIL setup: EspDefaultNvsPartition::take() failed: {e:?}");
            park();
        }
    };

    let _wifi = match connect_wifi(peripherals.modem, sysloop, nvs) {
        Ok(wifi) => wifi,
        Err(e) => {
            log::error!("FAIL setup: Wi-Fi association failed: {e:?}");
            park();
        }
    };

    // ---- Phase A: full handshakes, and does the printer offer a session at all? ----
    log::info!("=== phase A: {FULL_RUNS} full handshakes ===");
    let mut full_ms: Vec<u128> = Vec::new();
    let mut carried: Option<*mut sys::esp_tls_client_session_t> = None;

    for run in 1..=FULL_RUNS {
        log::info!("--- full run {run} of {FULL_RUNS} ---");
        match handshake(None) {
            Some(outcome) => {
                full_ms.push(outcome.elapsed_ms);
                match outcome.session {
                    Some(session) => {
                        // Keep the newest; free the one it replaces so a long phase A does
                        // not leak one session context per run.
                        if let Some(previous) = carried.replace(session) {
                            unsafe { sys::esp_tls_free_client_session(previous) };
                        }
                    }
                    None => log::warn!("    no resumable session from this handshake"),
                }
            }
            None => log::error!("    handshake failed, no timing from this run"),
        }
        std::thread::sleep(BETWEEN_RUNS);
    }

    let Some(session) = carried else {
        log::error!(
            "KEY RESULT: the printer never left a resumable session behind across {FULL_RUNS} \
             handshakes. Session resumption is not available against this model, so it cannot \
             reduce connect time and no change to EspIdfTlsConnector is worth making. With the \
             poll interval, Nagle and anchor trimming already ruled out, GitHub issue #160 \
             closes as irreducible at this layer."
        );
        report("full", &full_ms);
        park();
    };

    // ---- Phase B: replay the session ----
    log::info!("=== phase B: {RESUMED_RUNS} resumed handshakes ===");
    let mut resumed_ms: Vec<u128> = Vec::new();
    let mut current = session;

    for run in 1..=RESUMED_RUNS {
        log::info!("--- resumed run {run} of {RESUMED_RUNS} ---");
        match handshake(Some(current)) {
            Some(outcome) => {
                resumed_ms.push(outcome.elapsed_ms);
                // Take a fresh session each time: a printer may treat a ticket as single-use,
                // in which case reusing the original would silently fall back to a full
                // handshake and make phase B look like a failure of resumption itself.
                if let Some(next) = outcome.session {
                    unsafe { sys::esp_tls_free_client_session(current) };
                    current = next;
                }
            }
            None => log::error!("    resumed handshake failed, no timing from this run"),
        }
        std::thread::sleep(BETWEEN_RUNS);
    }

    unsafe { sys::esp_tls_free_client_session(current) };

    log::info!("================ issue #160 round 5 summary ================");
    let full_mean = report("full", &full_ms);
    let resumed_mean = report("resumed", &resumed_ms);

    match (full_mean, resumed_mean) {
        (Some(full), Some(resumed)) if full > resumed && full - resumed >= 200 => log::info!(
            "KEY RESULT: resumption saves ~{}ms per handshake ({full}ms -> {resumed}ms), which \
             clears the ~200ms floor rounds 3 and 4 established for this rig. Worth building \
             into EspIdfTlsConnector::connect by owning the esp_tls_cfg, and worth more than \
             once over: MQTT, FTPS and the camera each dial separately.",
            full - resumed
        ),
        (Some(full), Some(resumed)) => log::warn!(
            "KEY RESULT: no demonstrable saving ({full}ms full vs {resumed}ms resumed), inside \
             this rig's ~200ms noise floor. Read it with the id_len/ticket_len lines above: a \
             resumed handshake is one round trip with no ECDHE and no chain verification, so it \
             cannot take full-handshake time. A phase B run that still costs ~1.2s means the \
             peer declined to resume whatever it advertised. No packet capture is needed to \
             establish that much — the duration alone settles it."
        ),
        _ => log::error!(
            "KEY RESULT: not enough completed handshakes to compare. Read the failures above."
        ),
    }
    log::info!("===========================================================");

    park();
}

/// Runs one handshake, optionally installing a previously captured session, and returns the
/// caller-side wall time plus whatever session the printer left behind.
///
/// Drives `esp_tls_conn_new_sync` directly rather than `EspTls::negotiate` — see this file's
/// header for why. The loop mirrors `EspIdfTlsConnector::connect`: the fd is non-blocking,
/// `timeout_ms` is zero so each call takes exactly one handshake step, and the outer sleep
/// paces the retries.
fn handshake(session: Option<*mut sys::esp_tls_client_session_t>) -> Option<Outcome> {
    let stream = match std::net::TcpStream::connect((PRINTER_IP, PRINTER_TLS_PORT)) {
        Ok(stream) => stream,
        Err(e) => {
            log::error!("    TCP dial to {PRINTER_IP}:{PRINTER_TLS_PORT} failed: {e}");
            return None;
        }
    };
    if let Err(e) = stream.set_nonblocking(true) {
        log::error!("    set_nonblocking failed: {e}");
        return None;
    }
    // Matches what the crate now does on its own sockets.
    if let Err(e) = stream.set_nodelay(true) {
        log::warn!("    could not disable Nagle: {e}");
    }

    let tls = match EspTls::adopt(ProbeSocket(Some(stream))) {
        Ok(tls) => tls,
        Err(e) => {
            log::error!("    EspTls::adopt failed: {e}");
            return None;
        }
    };
    let handle = tls.context_handle();

    let mut cfg: sys::esp_tls_cfg = unsafe { core::mem::zeroed() };
    cfg.__bindgen_anon_1.cacert_buf = BBL_CA_ANCHOR.as_ptr();
    cfg.__bindgen_anon_2.cacert_bytes = BBL_CA_ANCHOR.len() as u32;
    // False for the adopted-socket path (issue #61): with it true, esp-tls never populates
    // its fd sets, because `adopt` enters at ESP_TLS_CONNECTING and skips the branch that
    // would have. The fd itself is O_NONBLOCK regardless, set above.
    cfg.non_block = false;
    cfg.timeout_ms = STEP_TIMEOUT_MS;
    cfg.client_session = session.unwrap_or(core::ptr::null_mut());

    let start = Instant::now();
    let result = loop {
        let ret = unsafe {
            sys::esp_tls_conn_new_sync(
                PRINTER_SERIAL.as_ptr() as *const core::ffi::c_char,
                PRINTER_SERIAL.len() as i32,
                PRINTER_TLS_PORT as i32,
                &cfg,
                handle,
            )
        };

        // The session may only be offered ONCE per connection, and this loop calls into the
        // handshake many times. `esp_mbedtls_handshake` runs
        // `mbedtls_ssl_set_session(&tls->ssl, &cfg->client_session->saved_session)` on every
        // entry (`esp_tls_mbedtls.c:281`), and mbedTLS rejects the second one:
        // `ssl_tls.c:1546` returns `MBEDTLS_ERR_SSL_FEATURE_UNAVAILABLE` once
        // `handshake->resume == 1`, which esp-tls flattens to a bare -1. That is invisible in
        // esp-tls's normal blocking use, where `esp_tls_conn_new_sync` completes the whole
        // handshake inside a single call — it only bites a caller that pins `timeout_ms = 0`
        // to take one step per call, which is exactly what GitHub issue #67 made this crate do.
        // Clearing the pointer after the first call leaves the session installed on the SSL
        // context while stopping esp-tls from re-offering it.
        cfg.client_session = core::ptr::null_mut();

        match ret {
            1 => break Ok(()),
            // The same three retryable outcomes as the crate's `is_would_block`.
            0 => {}
            r if r == sys::ESP_TLS_ERR_SSL_WANT_READ || r == sys::ESP_TLS_ERR_SSL_WANT_WRITE => {}
            other => break Err(other),
        }

        if start.elapsed() >= HANDSHAKE_TIMEOUT {
            break Err(sys::ESP_ERR_TIMEOUT);
        }
        std::thread::sleep(POLL_INTERVAL);
    };

    let elapsed_ms = start.elapsed().as_millis();

    if let Err(code) = result {
        log::error!("    handshake failed after {elapsed_ms}ms (esp-tls returned {code})");
        return None;
    }

    let exported = unsafe { sys::esp_tls_get_client_session(handle) };
    if exported.is_null() {
        log::info!("    handshake OK in {elapsed_ms}ms, no session could be exported");
        return Some(Outcome {
            elapsed_ms,
            session: None,
        });
    }

    // A non-NULL return does NOT mean the peer offered anything resumable. On TLS 1.2
    // `esp_tls_get_client_session` is a bare `mbedtls_ssl_get_session()`
    // (`esp_tls_mbedtls.c:257`), which succeeds for any negotiated session and never checks
    // that it carries resumption material. The session ID and the ticket are what a server
    // must actually supply, so read them rather than trusting the pointer: both empty means
    // the printer gave us nothing to resume *with*, which is a different finding from the
    // printer refusing a ticket we did offer.
    let saved = unsafe { &(*exported).saved_session };
    let id_len = saved.private_id_len;
    let ticket_len = saved.private_ticket_len;
    let resumable = id_len > 0 || ticket_len > 0;
    // The session ID prefix is the direct read on what the server decided. On ServerHello
    // mbedTLS keeps `resume = 1` only if the server echoed the *same* ID back
    // (`ssl_tls12_client.c:1324-1341`); otherwise it stores the server's new one. So a phase B
    // line whose id differs from the id phase A offered means the printer minted a fresh
    // session rather than resuming ours.
    log::info!(
        "    handshake OK in {elapsed_ms}ms, session id_len {id_len} id {}, ticket_len \
         {ticket_len}, ticket_lifetime {}s -> {}",
        hex8(&saved.private_id[..id_len.min(8)]),
        saved.private_ticket_lifetime,
        if resumable {
            "resumable"
        } else {
            "NOTHING to resume with"
        }
    );

    if !resumable {
        unsafe { sys::esp_tls_free_client_session(exported) };
        return Some(Outcome {
            elapsed_ms,
            session: None,
        });
    }

    Some(Outcome {
        elapsed_ms,
        session: Some(exported),
    })
}

/// Formats up to 8 bytes as hex, for eyeballing whether two session IDs are the same one.
fn hex8(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Logs per-run timings plus min/max/mean, returning the mean for the phase comparison.
fn report(label: &str, runs: &[u128]) -> Option<u128> {
    if runs.is_empty() {
        log::warn!("  {label}: no completed handshakes");
        return None;
    }
    let min = runs.iter().min().copied().unwrap_or(0);
    let max = runs.iter().max().copied().unwrap_or(0);
    let mean = runs.iter().sum::<u128>() / runs.len() as u128;
    log::info!("  {label}: {runs:?} -> min {min}ms, max {max}ms, mean {mean}ms");
    Some(mean)
}

fn connect_wifi(
    // `'static` because the returned `EspWifi<'static>` borrows it for the rest of `main`.
    modem: esp_idf_svc::hal::modem::Modem<'static>,
    sysloop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
) -> Result<BlockingWifi<EspWifi<'static>>, sys::EspError> {
    let mut wifi = BlockingWifi::wrap(EspWifi::new(modem, sysloop.clone(), Some(nvs))?, sysloop)?;

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: WIFI_SSID.try_into().expect("PROBE_WIFI_SSID too long"),
        password: WIFI_PASS.try_into().expect("PROBE_WIFI_PASS too long"),
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    }))?;

    wifi.start()?;
    wifi.connect()?;
    wifi.wait_netif_up()?;

    let ip = wifi.wifi().sta_netif().get_ip_info()?;
    // SSID deliberately not logged: the run transcript gets read back and pasted into an
    // issue, and the network name is the user's, not evidence for this investigation.
    log::info!("Wi-Fi associated, got IP {:?}", ip.ip);

    Ok(wifi)
}

/// ESP-IDF `main` is not meant to return; park so the monitor keeps the transcript on screen.
fn park() -> ! {
    loop {
        std::thread::sleep(Duration::from_secs(60));
    }
}
