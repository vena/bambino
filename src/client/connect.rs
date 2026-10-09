#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use core::future::Future;
use core::marker::PhantomData;
use core::time::Duration;

use crate::camera::CameraProtocol;
use crate::camera::binary::BinaryCameraStream;
use crate::error::Error;
use crate::ftps::FtpsClient;
use crate::identity::PrinterIdentity;
use crate::io::{
    AsyncIo, Raced, RawStreamFactory, SocketError, TimerProvider, TlsConnector, join3, race,
};
use crate::mqtt::MqttClient;

use super::PrinterClient;

/// Races `fut` against `connect_timeout` on `timer`; `None` (or a timer with no real clock) runs it unbounded.
///
/// Reuses the `race()` combinator `src/mqtt/client/{mod,frame}.rs`'s
/// `poll_wire`/`read_exact_packet` per-read deadline is built on, including its `has_real_clock()`
/// guard: under `DummyTimer` (`has_real_clock() == false`), `sleep()` completes instantly
/// regardless of duration, so racing against it unconditionally would make every connect attempt
/// look timed out instead of providing real protection — see `TimerProvider::has_real_clock`'s doc
/// comment.
async fn race_against_connect_timeout<TP, F, T, E>(
    timer: &TP,
    connect_timeout: Option<Duration>,
    fut: F,
) -> Result<T, E>
where
    TP: TimerProvider,
    F: Future<Output = Result<T, E>>,
    E: From<SocketError>,
{
    let Some(timeout) = connect_timeout.filter(|_| timer.has_real_clock()) else {
        return fut.await;
    };
    match race(fut, timer.sleep(timeout)).await {
        Raced::Left(result) => result,
        Raced::Right(r) => Err(E::from(crate::io::deadline_error(r))),
    }
}

/// Fails with [`Error::NotConfigured`] when `identity` has no address to dial — a `from_mqtt()` client.
///
/// Such a client was given a connected MQTT session and never an ip or access code; a channel it
/// has to dial itself can only be attached (`with_attached_ftps()`/`with_attached_camera()`).
fn require_dialable(identity: &PrinterIdentity, channel: &'static str) -> Result<(), Error> {
    if identity.ip.is_empty() || identity.access_code.is_empty() {
        return Err(Error::NotConfigured(
            format!(
                "{channel} needs the printer's ip and access code, which a from_mqtt() client \
                 doesn't have; use .with_attached_{channel}() instead"
            )
            .into(),
        ));
    }
    Ok(())
}

/// Dials, wraps in TLS and completes the MQTT handshake — the one MQTT connect sequence.
async fn dial_mqtt<RawIO, Tls, Factory>(
    factory: &Factory,
    tls: &Tls,
    identity: &PrinterIdentity,
    port: u16,
) -> Result<MqttClient<Tls::Stream>, Error>
where
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: RawStreamFactory<RawIO>,
{
    let raw = factory.dial(&identity.ip, port).await?;
    let stream = tls.connect(&identity.serial, raw).await?;
    MqttClient::connect(stream, &identity.serial, &identity.access_code).await
}

/// Dials, wraps in TLS and authenticates the binary camera stream — the one camera connect sequence.
///
/// The RTSPS check stays with the callers: `ensure_camera()` refuses an RTSPS model while
/// `connect_all()` reports it as not attempted (`.claude/rules/camera-trio.md`).
async fn dial_camera<RawIO, Tls, Factory>(
    factory: &Factory,
    tls: &Tls,
    identity: &PrinterIdentity,
    port: u16,
    max_frame_size: Option<usize>,
) -> Result<BinaryCameraStream<Tls::Stream>, Error>
where
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: RawStreamFactory<RawIO>,
{
    let raw = factory.dial(&identity.ip, port).await?;
    let stream = tls.connect(&identity.serial, raw).await?;
    let mut cam = BinaryCameraStream::new(stream);
    if let Some(max) = max_frame_size {
        cam = cam.with_max_frame_size(max);
    }
    cam.authenticate(&identity.access_code).await?;
    Ok(cam)
}

/// Dials and logs in the FTPS control channel over borrowed config — the one FTPS connect sequence.
///
/// Borrowed so a failed attempt leaves the config in place for a retry; [`install_ftps`] takes it
/// only once this has succeeded.
async fn dial_ftps<RawIO, Tls, Factory, FtpsTimer>(
    (tls, factory, timer): &(Tls, Factory, FtpsTimer),
    identity: &PrinterIdentity,
    port: u16,
    tls_version_check: crate::ftps::TlsVersionCheck,
) -> Result<(Tls::Stream, Vec<u8>), Error>
where
    RawIO: AsyncIo,
    Tls: TlsConnector<RawIO>,
    Factory: RawStreamFactory<RawIO>,
    FtpsTimer: TimerProvider,
{
    let raw = factory.dial(&identity.ip, port).await?;
    FtpsClient::<RawIO, Tls, Factory, FtpsTimer>::connect_control_stream(
        raw,
        tls,
        identity,
        timer,
        tls_version_check,
    )
    .await
}

/// Per-channel outcome of [`PrinterClient::connect_all`], one field per connection channel.
///
/// Each field distinguishes three states, which is the whole reason this is a struct rather
/// than a plain `Result`:
///
/// - `None` — the channel was **not attempted**. Either it was already connected, it was
///   never configured (no `.with_ftps()`/`.with_camera()`), or it cannot apply to this
///   printer at all (the camera on an RTSPS model). Not an error, and not a failure to
///   report to a user.
/// - `Some(Ok(()))` — connected, and the session is installed on the client.
/// - `Some(Err(e))` — that channel's own error, including its own
///   [`SocketError::TimedOut`] if it alone exceeded the connect timeout.
///
/// Every channel is reported independently and none of them short-circuits the others, so
/// partial success is a normal result rather than an edge case: a client whose MQTT session
/// came up and whose camera refused the connection has a usable MQTT session, and the
/// camera error is still visible instead of being swallowed or masking the success.
#[derive(Debug, Clone)]
pub struct ConnectAllOutcome {
    /// MQTT channel result — see the struct docs for what each state means.
    pub mqtt: Option<Result<(), Error>>,
    /// FTPS channel result — see the struct docs for what each state means.
    pub ftps: Option<Result<(), Error>>,
    /// Camera channel result — see the struct docs for what each state means.
    pub camera: Option<Result<(), Error>>,
}

/// One of [`PrinterClient`]'s connection channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    /// The MQTT command/telemetry session.
    Mqtt,
    /// The FTPS storage session.
    Ftps,
    /// The binary-JPEG camera stream.
    Camera,
}

impl ConnectAllOutcome {
    /// Every channel that was attempted and failed, with its error.
    ///
    /// A view over the per-channel fields for the "did everything I configured connect?"
    /// question; channels not attempted (`None`) aren't failures and don't appear.
    pub fn errors(&self) -> impl Iterator<Item = (Channel, &Error)> {
        [
            (Channel::Mqtt, &self.mqtt),
            (Channel::Ftps, &self.ftps),
            (Channel::Camera, &self.camera),
        ]
        .into_iter()
        .filter_map(|(channel, result)| match result {
            Some(Err(e)) => Some((channel, e)),
            _ => None,
        })
    }

    /// `Ok(())` if no attempted channel failed, else the first failure in MQTT, FTPS, camera order.
    ///
    /// For callers that treat a partial connect as a failed one. The per-channel fields stay
    /// available for those that don't.
    pub fn into_result(self) -> Result<(), Error> {
        [self.mqtt, self.ftps, self.camera]
            .into_iter()
            .flatten()
            .find_map(Result::err)
            .map_or(Ok(()), Err)
    }
}

impl<
    MqttRawIO,
    MqttTls,
    MqttFactory,
    Timer,
    FtpsRawIO,
    FtpsTls,
    FtpsFactory,
    FtpsTimer,
    CameraRawIO,
    CameraTls,
    CameraFactory,
>
    PrinterClient<
        MqttRawIO,
        MqttTls,
        MqttFactory,
        Timer,
        FtpsRawIO,
        FtpsTls,
        FtpsFactory,
        FtpsTimer,
        CameraRawIO,
        CameraTls,
        CameraFactory,
    >
where
    MqttRawIO: AsyncIo,
    MqttTls: TlsConnector<MqttRawIO>,
    MqttFactory: RawStreamFactory<MqttRawIO>,
    Timer: TimerProvider,
    FtpsRawIO: AsyncIo,
    FtpsTls: TlsConnector<FtpsRawIO>,
    FtpsFactory: RawStreamFactory<FtpsRawIO>,
    FtpsTimer: TimerProvider,
    CameraRawIO: AsyncIo,
    CameraTls: TlsConnector<CameraRawIO>,
    CameraFactory: RawStreamFactory<CameraRawIO>,
{
    /// Establishes the MQTT connection if not already connected.
    ///
    /// Short-circuits when `self.mqtt` is already `Some`. Otherwise runs `dial_mqtt` (dial, TLS,
    /// handshake) raced against the connect timeout.
    pub(super) async fn ensure_mqtt(&mut self) -> Result<(), Error> {
        if self.mqtt.is_some() {
            return Ok(());
        }
        let mqtt_client = race_against_connect_timeout(
            &self.timer,
            self.core.connect_timeout,
            dial_mqtt(
                &self.mqtt_factory,
                &self.mqtt_tls,
                &self.core.identity,
                self.core.mqtt_port,
            ),
        )
        .await?;
        self.install_mqtt(mqtt_client).await;
        Ok(())
    }

    /// Installs a freshly dialled MQTT session and runs every per-connection step, in order.
    ///
    /// The one install path for a session this client dialled itself (`ensure_mqtt()` and
    /// `connect_all()`), so a step added here cannot be missed by one of them — `connect_all()`
    /// once installed the session by hand and skipped both `begin_connection()` and the
    /// connect-time pushall. The reseed must follow `begin_connection()` and precede the pushall,
    /// which is the first id the new connection mints.
    async fn install_mqtt(&mut self, mqtt_client: MqttClient<MqttTls::Stream>) {
        self.mqtt = Some(mqtt_client);
        self.begin_connection();
        self.reseed_sequence_counter();
        self.publish_connect_pushall().await;
    }

    /// Marks an MQTT connection boundary, invalidating every cached value whose trustworthiness
    /// is scoped to a single connection.
    ///
    /// Bumping one counter is deliberately preferred over resetting fields one by one: the
    /// previous shape — `disconnect_mqtt()` clearing whatever its author remembered to clear —
    /// is exactly how `self.core.cache` came to survive a reconnect while `k_profile_primed` did not.
    /// A new connection-scoped cache field opts in by stamping `connection_generation` when it
    /// is written, and cannot be silently forgotten here.
    pub(crate) fn begin_connection(&mut self) {
        // Echoes addressed to the old session can no longer arrive; see
        // `CommandOutcome::ConnectionLost`.
        self.core.commands.connection_ended();
        self.core.k_profile_primed = false;
        self.core.connection_generation = self.core.connection_generation.wrapping_add(1);
    }

    /// Publishes a `pushall` immediately after a connection is established, refilling the
    /// telemetry cache that [`begin_connection()`](Self::begin_connection) just invalidated.
    ///
    /// Firmware broadcasts carry only *changed* fields [REF-MQTT-TELEMETRY], so a value that
    /// happens not to change across the reconnect may never be re-sent on its own — a full
    /// state dump is the only thing that reliably repopulates the cache. Every reference client
    /// does this from its own connect handler: BambuStudio (`GUI_App.cpp`, `:2209`,
    /// with `request_now = true` to bypass its own `REQUEST_PUSH_MIN_TIME` anti-burst floor),
    /// ha-bambulab (`pybambu/bambu_client.py`), and bambuddy
    /// (`services/bambu_mqtt.py`). None of them gates it on cache age, and none
    /// branches on model — don't route this through the quirks engine.
    ///
    /// Publishes through `publish_payload` rather than
    /// [`request_pushall()`](PrinterClient::request_pushall), because the latter re-enters
    /// `ensure_mqtt()`, which would make this an async recursion.
    ///
    /// **Deliberately non-fatal**, for the same reason as
    /// [`prime_firmware_version()`](Self::prime_firmware_version): a printer that never
    /// answers still has a usable session, and the accessors already report `None` rather
    /// than a stale value.
    async fn publish_connect_pushall(&mut self) {
        let seq = self.next_sequence_id();
        let request = crate::mqtt::PushAllRequest::new(seq);
        let payload = match super::serialize(&request) {
            Ok(payload) => payload,
            Err(e) => {
                log::debug!("connect-time pushall not sent ({e}); cache stays cold");
                return;
            }
        };
        let echo = crate::mqtt::client::echo_key(&payload);
        if let Err(e) = self.publish_payload(&payload, echo).await {
            log::debug!(
                "connect-time pushall failed ({e:?}); connection-scoped telemetry stays None until the printer reports"
            );
        }
    }

    /// Eagerly establishes the MQTT connection.
    ///
    /// Idempotent — returns `Ok(())` if already connected.
    pub async fn connect_mqtt(&mut self) -> Result<(), Error> {
        self.ensure_mqtt().await?;
        self.prime_firmware_version().await;
        Ok(())
    }

    /// Fetches and caches the printer's firmware version, ignoring failure.
    ///
    /// Several capabilities are gated on a minimum firmware release
    /// ([`ModelQuirks::ams_remote_drying_support`](crate::quirks::ModelQuirks::ams_remote_drying_support)
    /// and anything added beside it), and the version is connection-establishment data the same
    /// way the initial pushall is — bambuddy requests it from its own connect handler, next to
    /// `_request_push_all()` (`bambu_mqtt.py`). Doing it here means a connected client
    /// can answer capability questions without the caller knowing to ask for a version first.
    ///
    /// **Deliberately non-fatal.** A printer that never answers `get_version` still has a
    /// perfectly usable MQTT session, and failing the connect over an optional capability lookup
    /// would turn a missing nicety into an outage. The cached version simply stays `None`, which
    /// quirks read as "not asked" and resolve from their model rules — see
    /// [`ModelQuirks::ams_remote_drying_support`](crate::quirks::ModelQuirks::ams_remote_drying_support)
    /// for why that is the safe direction.
    ///
    /// Only the explicit connect paths call this. A caller relying on lazy connect — where
    /// `ensure_mqtt()` runs inside some other command — never pays this round trip, and gets the
    /// model-rule answer instead.
    pub(crate) async fn prime_firmware_version(&mut self) {
        if self.firmware_this_connection().is_some() {
            return;
        }
        if let Err(e) = self.get_version().await {
            log::debug!(
                "get_version during connect failed ({e:?}); firmware-gated capabilities will use model rules"
            );
        }
    }

    /// Returns whether the MQTT session is currently established.
    pub fn is_mqtt_connected(&self) -> bool {
        self.mqtt.is_some()
    }

    /// Injects a pre-connected [`MqttClient`] directly.
    ///
    /// Use this for test mocks or Embassy where the caller manages the MQTT connection,
    /// mirroring [`attach_camera()`](super::PrinterClient::attach_camera)/
    /// [`attach_ftps()`](super::PrinterClient::attach_ftps).
    ///
    /// A session already in the slot is closed first, as
    /// [`disconnect_mqtt()`](Self::disconnect_mqtt) does. The new one then gets every step a
    /// session this client dials itself gets: the connection-scoped cache is invalidated, the
    /// sequence counter is reseeded (under a timer with a real clock), and a `pushall` refills
    /// the cache.
    pub async fn attach_mqtt(&mut self, mqtt: MqttClient<MqttTls::Stream>) {
        self.close_mqtt_session().await;
        self.install_mqtt(mqtt).await;
    }

    /// Takes the MQTT session out of its slot, if any, and closes its TLS session before it drops.
    async fn close_mqtt_session(&mut self) {
        if let Some(mut mqtt) = self.mqtt.take()
            && let Err(e) = self.mqtt_tls.close(mqtt.stream_mut()).await
        {
            log::debug!("MQTT TLS close failed: {e:?}");
        }
    }

    /// Disconnects the MQTT session, if one exists, and clears it from the client.
    ///
    /// There is no protocol-level (MQTT DISCONNECT) teardown on `MqttClient` to call, but the
    /// TLS session underneath it is shut down properly before the slot is cleared —
    /// [`TlsConnector::close`] sends `close_notify` so the
    /// printer sees an orderly teardown rather than a truncated stream (GitHub issue #293).
    /// Failure there is logged and ignored: the connection is going away either way. Dropping
    /// the client is what releases the session's memory — on MbedTLS/embassy that is ~48 KB,
    /// freed in `Drop`, not in `close()`. Without this, a dead stream (a
    /// [`tick_zombie_check()`](crate::mqtt::MqttClient::tick_zombie_check)-detected
    /// zombie, a transport error) left `self.mqtt` stuck `Some(...)` forever, since
    /// `ensure_mqtt()`'s `is_some()` short-circuit kept handing back the same broken
    /// connection with no supported redial path.
    ///
    /// Idempotent. Reconnecting requires [`.attach_mqtt()`](Self::attach_mqtt) with a fresh
    /// `MqttClient` for a [`from_mqtt()`](PrinterClient::from_mqtt)-built client — its
    /// `PreConnected` factory's `dial()` always errors, so `ensure_mqtt()`'s lazy-dial fallback
    /// only recovers a `new()`-built client, never one built via `from_mqtt()`.
    pub async fn disconnect_mqtt(&mut self) {
        self.close_mqtt_session().await;
        self.begin_connection();
    }

    /// Sets a [`TimerProvider`] for wall-clock command-response timeouts.
    ///
    /// Consuming builder — works on both [`new()`](PrinterClient::new) and
    /// [`from_mqtt()`](PrinterClient::from_mqtt) construction paths. On a client already holding
    /// a session (`from_mqtt()`), the sequence counter is reseeded from the new timer's clock,
    /// which `from_mqtt()` itself cannot do under its `DummyTimer`.
    #[must_use]
    pub fn with_timer<NewTimer: TimerProvider>(
        self,
        timer: NewTimer,
    ) -> PrinterClient<
        MqttRawIO,
        MqttTls,
        MqttFactory,
        NewTimer,
        FtpsRawIO,
        FtpsTls,
        FtpsFactory,
        FtpsTimer,
        CameraRawIO,
        CameraTls,
        CameraFactory,
    > {
        let mut client = PrinterClient {
            mqtt: self.mqtt,
            ftps: self.ftps,
            ftps_config: self.ftps_config,
            camera: self.camera,
            camera_config: self.camera_config,
            mqtt_tls: self.mqtt_tls,
            mqtt_factory: self.mqtt_factory,
            timer,
            core: self.core,
            _mqtt_raw_io: PhantomData,
            _camera_raw_io: PhantomData,
        };
        // `from_mqtt()` installs its session under `DummyTimer`, where the reseed is skipped;
        // this is the first point a real clock is available to it (#346).
        if client.mqtt.is_some() {
            client.reseed_sequence_counter();
        }
        client
    }

    /// Overrides the default MQTT port (8883).
    #[must_use]
    pub fn with_mqtt_port(mut self, port: u16) -> Self {
        self.core.mqtt_port = port;
        self
    }

    /// Sets the bound on each channel's dial+TLS+handshake; `None` disables it.
    ///
    /// The default is [`DEFAULT_CONNECT_TIMEOUT`](super::DEFAULT_CONNECT_TIMEOUT) (10s). Needs a
    /// real clock ([`with_timer()`](Self::with_timer)) to fire. Keeps the type parameters;
    /// chain onto any construction path.
    ///
    /// This is the only connect budget on every backend. `EspIdfTlsConnector` has its own
    /// handshake deadline for direct use, but it is disabled unless set, so it doesn't cap this
    /// one; leave it unset under `PrinterClient`.
    #[must_use]
    pub fn with_connect_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.core.connect_timeout = timeout;
        self
    }

    /// Configures FTPS for lazy connection on first storage method call.
    ///
    /// Consuming builder — changes the `FtpsRawIO`, `FtpsTls`, `FtpsFactory`, and `FtpsTimer`
    /// type parameters. The FTPS [`TlsConnector`] is independent from MQTT's (some models
    /// require different TLS settings for FTPS, e.g. `TlsVersions::Tls12Only`). `timer` is
    /// constructed fresh by the caller (e.g. `TokioTimer::new()`) — `FtpsClient` owns it
    /// independently of `PrinterClient`'s own `Timer`, since `PrinterClient::ftps()` hands
    /// out direct `&mut FtpsClient` access rather than mediating every FTPS call itself,
    /// so there's no call site to thread `self.timer` through the way MQTT/camera do.
    ///
    /// Call [`disconnect_ftps()`](Self::disconnect_ftps) first on a client with a
    /// connected FTPS session: this builder is synchronous and cannot close it, so the session is
    /// dropped without `close_notify` (see `.claude/rules/tls-session-teardown.md`).
    ///
    /// On a [`from_mqtt()`](PrinterClient::from_mqtt) client, which has no ip or access code to
    /// dial with, the first FTPS call returns [`Error::NotConfigured`]; use
    /// [`with_attached_ftps()`](Self::with_attached_ftps) there.
    #[must_use]
    pub fn with_ftps<NewFtpsRawIO, NewFtpsTls, NewFtpsFactory, NewFtpsTimer>(
        self,
        tls: NewFtpsTls,
        factory: NewFtpsFactory,
        timer: NewFtpsTimer,
    ) -> PrinterClient<
        MqttRawIO,
        MqttTls,
        MqttFactory,
        Timer,
        NewFtpsRawIO,
        NewFtpsTls,
        NewFtpsFactory,
        NewFtpsTimer,
        CameraRawIO,
        CameraTls,
        CameraFactory,
    >
    where
        NewFtpsRawIO: AsyncIo,
        NewFtpsTls: TlsConnector<NewFtpsRawIO>,
        NewFtpsFactory: RawStreamFactory<NewFtpsRawIO>,
        NewFtpsTimer: TimerProvider,
    {
        PrinterClient {
            mqtt: self.mqtt,
            ftps: None,
            ftps_config: Some((tls, factory, timer)),
            camera: self.camera,
            camera_config: self.camera_config,
            mqtt_tls: self.mqtt_tls,
            mqtt_factory: self.mqtt_factory,
            timer: self.timer,
            core: self.core,
            _mqtt_raw_io: PhantomData,
            _camera_raw_io: PhantomData,
        }
    }

    /// Overrides the default FTPS port (990).
    #[must_use]
    pub fn with_ftps_port(mut self, port: u16) -> Self {
        self.core.ftps_port = port;
        self
    }

    /// Overrides the default `false` for `FtpsClient`'s TLS-1.2-enforcement bypass.
    ///
    /// Rarely needed. Every backend can now *report* the negotiated version, so
    /// `require_tls_1_2_if_enforced` passes on its own whenever a P2S/X2D actually negotiates
    /// TLS 1.2 — see `src/ftps/CLAUDE.md` and `src/io/CLAUDE.md`. What differs between
    /// backends is the ability to *cap* the peer at 1.2: only `tokio` has that knob
    /// (`TlsVersions::Tls12Only` on `TokioTlsConnector::verified`/`unverified`). `esp-idf` and
    /// `embassy` set no maximum
    /// version — upstream exposes none on ESP-IDF, and this crate sets only `min_version` on
    /// embassy — so against a printer that insisted on TLS 1.3 they fail closed, and this
    /// bypass is the only way through. It skips the version check only; certificate
    /// verification is configured on the `TlsConnector` and is unaffected.
    /// Keeps the type parameters; chain onto any construction path.
    #[must_use]
    pub fn with_ftps_allow_unverified_tls_1_2(mut self, allow: bool) -> Self {
        self.core.ftps_tls_version_check = if allow {
            crate::ftps::TlsVersionCheck::Bypass
        } else {
            crate::ftps::TlsVersionCheck::Enforce
        };
        self
    }

    /// Establishes the FTPS connection if not already connected.
    ///
    /// Short-circuits when `self.ftps` is already `Some`. Otherwise runs `dial_ftps` over the
    /// borrowed `ftps_config`, raced against the connect timeout. `ftps_config` is only consumed
    /// once that attempt has actually succeeded — a failed attempt, including a connect timeout
    /// on a slow LAN, leaves it intact so the next call retries instead of permanently reporting
    /// "not configured". A poisoned session is disconnected first (its parts return to
    /// `ftps_config`) and redialed.
    pub(super) async fn ensure_ftps(&mut self) -> Result<(), Error> {
        match &self.ftps {
            Some(client) if client.is_poisoned() => self.disconnect_ftps().await,
            Some(_) => return Ok(()),
            None => {}
        }
        let config = self.ftps_config.as_ref().ok_or_else(|| {
            Error::NotConfigured(
                "FTPS — call .with_ftps(), .attach_ftps() or .with_attached_ftps()".into(),
            )
        })?;
        require_dialable(&self.core.identity, "ftps")?;
        let (control_stream, fill_buf) = race_against_connect_timeout(
            &self.timer,
            self.core.connect_timeout,
            dial_ftps(
                config,
                &self.core.identity,
                self.core.ftps_port,
                self.core.ftps_tls_version_check,
            ),
        )
        .await?;
        self.install_ftps(control_stream, fill_buf);
        Ok(())
    }

    /// Builds the [`FtpsClient`] from a control stream `dial_ftps` returned, consuming `ftps_config`.
    ///
    /// Only called after a successful dial, so `ftps_config` is still present.
    fn install_ftps(&mut self, control_stream: FtpsTls::Stream, fill_buf: Vec<u8>) {
        let (tls, factory, timer) = self
            .ftps_config
            .take()
            .expect("dial_ftps borrowed ftps_config, so it is still configured");
        self.ftps = Some(FtpsClient::from_control_stream(
            control_stream,
            tls,
            factory,
            &self.core.identity,
            timer,
            self.core.ftps_tls_version_check,
            fill_buf,
        ));
    }

    /// Eagerly establishes the FTPS connection.
    ///
    /// Idempotent — returns `Ok(())` if already connected.
    pub async fn connect_ftps(&mut self) -> Result<(), Error> {
        self.ensure_ftps().await
    }

    /// Returns whether a usable FTPS session is established (one that a transport failure poisoned is not).
    pub fn is_ftps_connected(&self) -> bool {
        self.ftps
            .as_ref()
            .is_some_and(|client| !client.is_poisoned())
    }

    /// Establishes the camera connection if not already connected.
    ///
    /// Returns [`Error::ModelMismatch`] immediately, without dialing, for RTSPS models — those
    /// use `camera::rtsps::build_rtsps_url()` instead and have no `PrinterClient`-managed
    /// connection state. Otherwise runs `dial_camera` raced against the connect timeout,
    /// mirroring `ensure_ftps()`.
    pub(super) async fn ensure_camera(&mut self) -> Result<(), Error> {
        if self.quirks().camera_protocol() != CameraProtocol::BinaryJpeg {
            return Err(Error::ModelMismatch(
                "this model streams its camera over RTSPS — use camera::rtsps::build_rtsps_url()"
                    .into(),
            ));
        }
        if self.camera.is_some() {
            return Ok(());
        }
        let (tls, factory) = self.camera_config.as_ref().ok_or_else(|| {
            Error::NotConfigured(
                "camera — call .with_camera(), .attach_camera() or .with_attached_camera()".into(),
            )
        })?;
        require_dialable(&self.core.identity, "camera")?;
        let camera_stream = race_against_connect_timeout(
            &self.timer,
            self.core.connect_timeout,
            dial_camera(
                factory,
                tls,
                &self.core.identity,
                self.core.camera_port,
                self.core.camera_max_frame_size,
            ),
        )
        .await?;
        // `camera_config` is deliberately *not* cleared here. Unlike `ftps_config`, whose
        // connector is moved into the `FtpsClient`, nothing is consumed by a camera connect —
        // the borrow above is `as_ref()` only. Keeping it is what lets `disconnect_camera()`
        // close the TLS session through the connector (GitHub issue #293), and it makes a
        // disconnect/reconnect cycle work instead of permanently reporting "not configured".
        self.camera = Some(camera_stream);
        Ok(())
    }

    /// Eagerly establishes the camera connection.
    ///
    /// Idempotent — returns `Ok(())` if already connected.
    pub async fn connect_camera(&mut self) -> Result<(), Error> {
        self.ensure_camera().await
    }

    /// Returns whether the camera session is currently established.
    pub fn is_camera_connected(&self) -> bool {
        self.camera.is_some()
    }

    /// Connects every configured channel concurrently, overlapping their TLS handshakes.
    ///
    /// Same end state as calling [`connect_mqtt()`](Self::connect_mqtt),
    /// [`connect_ftps()`](Self::connect_ftps) and [`connect_camera()`](Self::connect_camera)
    /// in sequence, but the three dial+TLS sequences are interleaved on this task instead of
    /// running one after another, and the result is reported per channel via
    /// [`ConnectAllOutcome`] rather than as a single `Result`.
    ///
    /// # Which channels are attempted
    ///
    /// Configuration *is* the selection — there is no channel argument. A channel is dialled
    /// only when it is configured and applicable, and is otherwise reported as `None`
    /// (not attempted) rather than as an error:
    ///
    /// - **MQTT** — attempted unless already connected.
    /// - **FTPS** — attempted only if `.with_ftps()` supplied a config and it is not already
    ///   connected. A consumer that never configured FTPS simply gets `None`.
    /// - **Camera** — attempted only if `.with_camera()` supplied a config *and* the model's
    ///   [`CameraProtocol`] is `BinaryJpeg`. Note the deliberate difference from
    ///   [`connect_camera()`](Self::connect_camera), which returns
    ///   [`Error::ModelMismatch`] on an RTSPS model: here an RTSPS camera is a channel
    ///   that does not apply to this printer, not a failure, so reporting it as an error
    ///   would hand every P2S/X2D consumer a guaranteed `Err` on an otherwise clean connect.
    ///   Those models use `camera::rtsps::build_rtsps_url()` and have no client-managed
    ///   connection to establish.
    ///
    /// # Timeouts
    ///
    /// The connect timeout is applied **per channel**, matching the individual
    /// `ensure_*` methods, so a slow or unreachable camera can never cause an otherwise
    /// healthy MQTT dial to be reported as timed out. Because the channels run concurrently
    /// the worst-case wall clock for the whole call is still one timeout, not three. A
    /// shared deadline around the joined future was rejected precisely because it cannot
    /// express partial success: it would discard an already-completed MQTT session when a
    /// hung camera pushed the *combined* future past the deadline.
    ///
    /// # Cost
    ///
    /// This future holds all three handshakes alive at once, so it costs roughly 4x the
    /// stack of connecting individually — measured on an ESP32-C6, 20296 bytes against a
    /// 4808-byte peak for the largest single `connect_*`, in exchange for ~1.3s. Irrelevant
    /// on desktop; on Embassy, where task stacks are sized up front, connect one at a time
    /// if stack is tighter than time.
    ///
    /// # Failure isolation
    ///
    /// A channel that fails installs nothing and leaves its config intact, so a later
    /// `connect_*`/`ensure_*` call retries it — the same "a failed attempt must not
    /// permanently report 'not configured'" rule the sequential paths follow. One channel's
    /// failure never prevents another from being installed.
    ///
    /// # Why this exists
    ///
    /// A handshake against a Bambu printer is dominated by waiting on the peer (~800ms,
    /// measured on an ESP32-C6 against a P1S and reproduced from a laptop on the same LAN,
    /// so it is the printer being slow rather than the client). That wait overlaps freely;
    /// only the smaller per-handshake compute term still serialises on a single core.
    /// Connecting three channels therefore costs roughly one peer wait plus three compute
    /// terms instead of three of each. TLS session resumption would have attacked the peer
    /// term directly, but the printer declines to resume its own session IDs, so overlapping
    /// the waits is the available lever.
    pub async fn connect_all(&mut self) -> ConnectAllOutcome {
        let timer = &self.timer;
        let timeout = self.core.connect_timeout;
        let identity = &self.core.identity;

        let mqtt_wanted = self.mqtt.is_none();
        let (mqtt_factory, mqtt_tls) = (&self.mqtt_factory, &self.mqtt_tls);
        let mqtt_port = self.core.mqtt_port;
        let mqtt_fut = async move {
            if !mqtt_wanted {
                return None;
            }
            let dial = dial_mqtt(mqtt_factory, mqtt_tls, identity, mqtt_port);
            Some(race_against_connect_timeout(timer, timeout, dial).await)
        };

        // `as_ref()` only — `ftps_config` is consumed after the join, and only on success,
        // so a failed attempt still leaves it available for a retry.
        let ftps_slot = if self.ftps.is_none() {
            self.ftps_config.as_ref()
        } else {
            None
        };
        let ftps_port = self.core.ftps_port;
        let tls_version_check = self.core.ftps_tls_version_check;
        let ftps_fut = async move {
            let config = ftps_slot?;
            Some(
                async {
                    require_dialable(identity, "ftps")?;
                    let dial = dial_ftps(config, identity, ftps_port, tls_version_check);
                    race_against_connect_timeout(timer, timeout, dial).await
                }
                .await,
            )
        };

        let camera_slot = if self.camera.is_none()
            && identity.model.quirks().camera_protocol() == CameraProtocol::BinaryJpeg
        {
            self.camera_config.as_ref()
        } else {
            None
        };
        let camera_port = self.core.camera_port;
        let max_frame_size = self.core.camera_max_frame_size;
        let camera_fut = async move {
            let (tls, factory) = camera_slot?;
            Some(
                async {
                    require_dialable(identity, "camera")?;
                    let dial = dial_camera(factory, tls, identity, camera_port, max_frame_size);
                    race_against_connect_timeout(timer, timeout, dial).await
                }
                .await,
            )
        };

        let (mqtt_res, ftps_res, camera_res) = join3(mqtt_fut, ftps_fut, camera_fut).await;

        // Every borrow above ends with the joined future; installing the results is the
        // only part that needs `&mut self`, which is why the three `ensure_*` methods did
        // not have to be restructured to make this concurrent.
        let mqtt = match mqtt_res {
            None => None,
            Some(Err(e)) => Some(Err(e)),
            Some(Ok(client)) => {
                self.install_mqtt(client).await;
                Some(Ok(()))
            }
        };

        let ftps = match ftps_res {
            None => None,
            Some(Err(e)) => Some(Err(e)),
            Some(Ok((control_stream, fill_buf))) => {
                self.install_ftps(control_stream, fill_buf);
                Some(Ok(()))
            }
        };

        let camera = match camera_res {
            None => None,
            Some(Err(e)) => Some(Err(e)),
            Some(Ok(cam)) => {
                // Not cleared, for the same reason as in `ensure_camera()` above.
                self.camera = Some(cam);
                Some(Ok(()))
            }
        };

        // Only once the MQTT session is installed — `prime_firmware_version` publishes, and
        // it is non-fatal, so a failure here leaves the outcome above untouched.
        if matches!(mqtt, Some(Ok(()))) {
            self.prime_firmware_version().await;
        }

        ConnectAllOutcome { mqtt, ftps, camera }
    }

    /// Configures the binary-JPEG camera for lazy connection on first camera method call.
    ///
    /// Consuming builder — changes the `CameraRawIO`, `CameraTls`, and `CameraFactory` type
    /// parameters. Independent of MQTT's and FTPS's connectors, mirroring `.with_ftps()`.
    ///
    /// Call [`disconnect_camera()`](Self::disconnect_camera) first on a client with a connected
    /// camera session, for the same reason as `.with_ftps()`. On a
    /// [`from_mqtt()`](PrinterClient::from_mqtt) client the first camera call returns
    /// [`Error::NotConfigured`]; use [`with_attached_camera()`](Self::with_attached_camera) there.
    #[must_use]
    pub fn with_camera<NewCameraRawIO, NewCameraTls, NewCameraFactory>(
        self,
        tls: NewCameraTls,
        factory: NewCameraFactory,
    ) -> PrinterClient<
        MqttRawIO,
        MqttTls,
        MqttFactory,
        Timer,
        FtpsRawIO,
        FtpsTls,
        FtpsFactory,
        FtpsTimer,
        NewCameraRawIO,
        NewCameraTls,
        NewCameraFactory,
    >
    where
        NewCameraRawIO: AsyncIo,
        NewCameraTls: TlsConnector<NewCameraRawIO>,
        NewCameraFactory: RawStreamFactory<NewCameraRawIO>,
    {
        PrinterClient {
            mqtt: self.mqtt,
            ftps: self.ftps,
            ftps_config: self.ftps_config,
            camera: None,
            camera_config: Some((tls, factory)),
            mqtt_tls: self.mqtt_tls,
            mqtt_factory: self.mqtt_factory,
            timer: self.timer,
            core: self.core,
            _mqtt_raw_io: PhantomData,
            _camera_raw_io: PhantomData,
        }
    }

    /// Installs a camera stream the caller connected, changing the camera type parameters to match it.
    ///
    /// The attach path for a [`from_mqtt()`](PrinterClient::from_mqtt) client: its camera slots
    /// are fixed to placeholder types, so [`attach_camera()`](Self::attach_camera) cannot take a
    /// real stream there, and [`with_camera()`](Self::with_camera) needs the ip/access code such
    /// a client lacks. `tls` is the connector that produced the stream; it is kept so
    /// [`disconnect_camera()`](Self::disconnect_camera) can send `close_notify`. There is no
    /// dialer, so after a disconnect the next camera call returns
    /// [`SocketError::NotConnected`] until a camera is
    /// attached again.
    ///
    /// Call [`disconnect_camera()`](Self::disconnect_camera) first on a client with a connected
    /// camera session, for the same reason as `.with_ftps()`.
    #[must_use]
    // The return type is `Self` with three parameters swapped, spelled out as every
    // type-changing builder here does; the nested `PreConnected<_>` just tips it over the lint.
    #[allow(clippy::type_complexity)]
    pub fn with_attached_camera<NewCameraRawIO, NewCameraTls>(
        self,
        tls: NewCameraTls,
        camera: BinaryCameraStream<NewCameraTls::Stream>,
    ) -> PrinterClient<
        MqttRawIO,
        MqttTls,
        MqttFactory,
        Timer,
        FtpsRawIO,
        FtpsTls,
        FtpsFactory,
        FtpsTimer,
        NewCameraRawIO,
        NewCameraTls,
        super::PreConnected<NewCameraRawIO>,
    >
    where
        NewCameraRawIO: AsyncIo,
        NewCameraTls: TlsConnector<NewCameraRawIO>,
    {
        PrinterClient {
            mqtt: self.mqtt,
            ftps: self.ftps,
            ftps_config: self.ftps_config,
            camera: Some(camera),
            camera_config: Some((tls, super::PreConnected(PhantomData))),
            mqtt_tls: self.mqtt_tls,
            mqtt_factory: self.mqtt_factory,
            timer: self.timer,
            core: self.core,
            _mqtt_raw_io: PhantomData,
            _camera_raw_io: PhantomData,
        }
    }

    /// Installs an FTPS client the caller connected, changing the FTPS type parameters to match it.
    ///
    /// The FTPS counterpart of [`with_attached_camera()`](Self::with_attached_camera), for a
    /// [`from_mqtt()`](PrinterClient::from_mqtt) client whose FTPS slots are placeholders.
    /// [`FtpsClient`] carries its own connector, so
    /// [`disconnect_ftps()`](Self::disconnect_ftps) closes it as usual. No FTPS
    /// configuration is kept, so after a disconnect [`ftps()`](Self::ftps) reports FTPS
    /// as not configured until a client is attached again.
    ///
    /// Call [`disconnect_ftps()`](Self::disconnect_ftps) first on a client with a
    /// connected FTPS session, for the same reason as `.with_ftps()`.
    #[must_use]
    pub fn with_attached_ftps<NewFtpsRawIO, NewFtpsTls, NewFtpsFactory, NewFtpsTimer>(
        self,
        ftps_client: FtpsClient<NewFtpsRawIO, NewFtpsTls, NewFtpsFactory, NewFtpsTimer>,
    ) -> PrinterClient<
        MqttRawIO,
        MqttTls,
        MqttFactory,
        Timer,
        NewFtpsRawIO,
        NewFtpsTls,
        NewFtpsFactory,
        NewFtpsTimer,
        CameraRawIO,
        CameraTls,
        CameraFactory,
    >
    where
        NewFtpsRawIO: AsyncIo,
        NewFtpsTls: TlsConnector<NewFtpsRawIO>,
        NewFtpsFactory: RawStreamFactory<NewFtpsRawIO>,
        NewFtpsTimer: TimerProvider,
    {
        PrinterClient {
            mqtt: self.mqtt,
            ftps: Some(ftps_client),
            ftps_config: None,
            camera: self.camera,
            camera_config: self.camera_config,
            mqtt_tls: self.mqtt_tls,
            mqtt_factory: self.mqtt_factory,
            timer: self.timer,
            core: self.core,
            _mqtt_raw_io: PhantomData,
            _camera_raw_io: PhantomData,
        }
    }

    /// Overrides the default camera port (6000, binary-JPEG only).
    #[must_use]
    pub fn with_camera_port(mut self, port: u16) -> Self {
        self.core.camera_port = port;
        self
    }

    /// Overrides the default maximum accepted camera frame size (see `BinaryCameraStream::with_max_frame_size`).
    #[must_use]
    pub fn with_camera_max_frame_size(mut self, bytes: usize) -> Self {
        self.core.camera_max_frame_size = Some(bytes);
        self
    }
}
