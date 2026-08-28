use core::future::Future;
use core::marker::PhantomData;

use crate::camera::CameraProtocol;
use crate::camera::binary::BinaryCameraStream;
use crate::error::Error;
use crate::ftps::FtpsClient;
use crate::io::{
    AsyncIo, Raced, RawStreamFactory, SocketError, TimerProvider, TlsConnector, join3, race,
};
use crate::mqtt::MqttClient;

use super::PrinterClient;

/// Races `fut` against a `connect_timeout_secs`-second deadline on `timer`, used by `ensure_mqtt()`/`ensure_ftps()` to bound their two-step dial+connect sequences.
/// Reuses the `race()` combinator `src/mqtt/client/{mod,frame}.rs`'s
/// `poll_wire`/`read_exact_packet` per-read deadline is built on, including its `has_real_clock()`
/// guard: under `DummyTimer` (`has_real_clock() == false`), `sleep()` completes instantly
/// regardless of duration, so racing against it unconditionally would make every connect attempt
/// look timed out instead of providing real protection — see `TimerProvider::has_real_clock`'s doc
/// comment.
///
/// `connect_timeout_secs == 0` also skips the race, matching `set_command_timeout`'s "0 disables
/// the timeout" convention — without this, `timer.sleep(Duration::from_secs(0))` resolves
/// effectively instantly and wins the race against the dial+TLS+handshake future on nearly every
/// attempt, since that future essentially never completes synchronously on its first poll. That
/// would make `0` mean "always fail immediately" instead of "disabled," the opposite of the
/// sibling `command_timeout_secs` field's documented behavior.
async fn race_against_connect_timeout<TP, F, T, E>(
    timer: &TP,
    connect_timeout_secs: u64,
    fut: F,
) -> Result<T, E>
where
    TP: TimerProvider,
    F: Future<Output = Result<T, E>>,
    E: From<SocketError>,
{
    if !timer.has_real_clock() || connect_timeout_secs == 0 {
        return fut.await;
    }
    let sleep_fut = timer.sleep(core::time::Duration::from_secs(connect_timeout_secs));
    match race(fut, sleep_fut).await {
        Raced::Left(result) => result,
        Raced::Right(_) => Err(E::from(SocketError::TimedOut)),
    }
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
    /// Short-circuits when `self.mqtt` is already `Some`. Otherwise, dials a raw stream via
    /// `self.mqtt_factory.dial()`, wraps it in TLS via `self.mqtt_tls.connect()`, then calls
    /// `MqttClient::connect()` — the whole dial+TLS+handshake sequence is raced against
    /// `self.connect_timeout_secs`.
    pub(super) async fn ensure_mqtt(&mut self) -> Result<(), Error> {
        if self.mqtt.is_some() {
            return Ok(());
        }
        let mqtt_client =
            race_against_connect_timeout(&self.timer, self.connect_timeout_secs, async {
                let raw = self
                    .mqtt_factory
                    .dial(&self.identity.ip, self.mqtt_port)
                    .await?;
                let stream = self.mqtt_tls.connect(&self.identity.serial, raw).await?;
                MqttClient::connect(stream, &self.identity).await
            })
            .await?;
        self.mqtt = Some(mqtt_client);
        // Reseed from wall-clock time so two independent sessions connecting to the
        // same printer don't start from the same fixed counter and risk colliding
        // sequence IDs while both have in-flight requests. Skipped under a timer with
        // no real clock (e.g. DummyTimer, always 0) — reseeding to a constant would
        // recreate exactly the collision this exists to prevent, and existing tests
        // rely on the deterministic default sequence when no real timer is chained.
        if self.timer.has_real_clock() {
            self.sequence_counter =
                crate::mqtt::commands::clamp_task_id(self.timer.now_millis()) as u64;
        }
        Ok(())
    }

    /// Eagerly establishes the MQTT connection.
    ///
    /// Idempotent — returns `Ok(())` if already connected.
    pub async fn connect_mqtt(&mut self) -> Result<(), Error> {
        self.ensure_mqtt().await
    }

    /// Returns whether the MQTT session is currently established.
    pub fn is_mqtt_connected(&self) -> bool {
        self.mqtt.is_some()
    }

    /// Injects a pre-connected [`MqttClient`] directly.
    ///
    /// Use this for test mocks or Embassy where the caller manages the MQTT connection,
    /// mirroring [`attach_camera()`](super::PrinterClient::attach_camera)/
    /// [`attach_storage()`](super::PrinterClient::attach_storage).
    pub fn attach_mqtt(&mut self, mqtt: MqttClient<MqttTls::Stream>) {
        self.mqtt = Some(mqtt);
        self.k_profile_primed = false;
    }

    /// Disconnects the MQTT session, if one exists, and clears it from the client.
    ///
    /// There is no protocol-level teardown on `MqttClient` to call — this just clears
    /// the slot, mirroring `disconnect_camera()`. Without this, a dead stream (a
    /// [`tick_zombie_check()`](crate::mqtt::MqttClient::tick_zombie_check)-detected
    /// zombie, a transport error) left `self.mqtt` stuck `Some(...)` forever, since
    /// `ensure_mqtt()`'s `is_some()` short-circuit kept handing back the same broken
    /// connection with no supported redial path.
    ///
    /// Idempotent. Reconnecting requires [`.attach_mqtt()`](Self::attach_mqtt) with a fresh
    /// `MqttClient` for a [`from_mqtt()`](PrinterClient::from_mqtt)-built client — its
    /// `PreConnected` factory's `dial()` always errors, so `ensure_mqtt()`'s lazy-dial fallback
    /// only recovers a `connect()`-built client, never one built via `from_mqtt()`.
    pub async fn disconnect_mqtt(&mut self) -> Result<(), Error> {
        self.mqtt = None;
        self.k_profile_primed = false;
        Ok(())
    }

    /// Sets a [`TimerProvider`] for wall-clock command-response timeouts.
    ///
    /// Consuming builder — works on both [`new()`](PrinterClient::new) and
    /// [`from_mqtt()`](PrinterClient::from_mqtt) construction paths.
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
        PrinterClient {
            mqtt: self.mqtt,
            ftps: self.ftps,
            ftps_config: self.ftps_config,
            camera: self.camera,
            camera_config: self.camera_config,
            mqtt_tls: self.mqtt_tls,
            mqtt_factory: self.mqtt_factory,
            timer,
            identity: self.identity,
            sequence_counter: self.sequence_counter,
            k_profile_primed: self.k_profile_primed,
            cache: self.cache,
            command_timeout_secs: self.command_timeout_secs,
            connect_timeout_secs: self.connect_timeout_secs,
            mqtt_port: self.mqtt_port,
            ftps_port: self.ftps_port,
            ftps_allow_unverified_tls_1_2: self.ftps_allow_unverified_tls_1_2,
            camera_port: self.camera_port,
            camera_max_frame_size: self.camera_max_frame_size,
            _mqtt_raw_io: PhantomData,
            _camera_raw_io: PhantomData,
        }
    }

    /// Overrides the default MQTT port (8883).
    #[must_use]
    pub fn with_mqtt_port(mut self, port: u16) -> Self {
        self.mqtt_port = port;
        self
    }

    /// Overrides the default connect-timeout deadline (10s) that bounds `ensure_mqtt()`/`ensure_ftps()`'s combined dial+TLS-connect sequence.
    /// Passing `0` disables the timeout entirely, matching `set_command_timeout`'s "0 disables"
    /// convention. Non-consuming — chain onto any construction path.
    ///
    /// On ESP-IDF, this budget is structurally independent from `EspIdfTlsConnector`'s own
    /// internal handshake timeout (default 10s) — the connector is an opaque generic by the
    /// time it reaches `PrinterClient::new()`, so this outer setting can't see or influence it.
    /// Set `EspIdfTlsConnector::with_connect_timeout` directly and keep the two in sync,
    /// including the `0` case (both treat `0` as "disabled", but neither number implies the
    /// other). Not an issue on `tokio`/`embassy`, where the handshake is bounded solely by
    /// this outer race.
    #[must_use]
    pub fn with_connect_timeout(mut self, secs: u64) -> Self {
        self.connect_timeout_secs = secs;
        self
    }

    /// Configures FTPS for lazy connection on first storage method call.
    ///
    /// Consuming builder — changes the `FtpsRawIO`, `FtpsTls`, `FtpsFactory`, and `FtpsTimer`
    /// type parameters. The FTPS [`TlsConnector`] is independent from MQTT's (some models
    /// require different TLS settings for FTPS, e.g. `force_tls_1_2`). `timer` is
    /// constructed fresh by the caller (e.g. `TokioTimer::new()`) — `FtpsClient` owns it
    /// independently of `PrinterClient`'s own `Timer`, since `PrinterClient::storage()` hands
    /// out direct `&mut FtpsClient` access rather than mediating every FTPS call itself,
    /// so there's no call site to thread `self.timer` through the way MQTT/camera do.
    ///
    /// Must not be called on a client with an already-connected FTPS session — the existing
    /// connection is dropped (not explicitly disconnected) when the new struct is built.
    /// Functionally safe (LAN-only TCP/TLS, `Drop`-based teardown), but callers should
    /// disconnect first if they want an explicit, orderly teardown.
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
        // `from_mqtt()`-constructed clients have empty `ip`/`access_code` (no host
        // config was ever supplied), which would otherwise fail opaquely at actual FTPS
        // connect time — panic here instead, at the builder call site, with a clear message
        // pointing at the real cause.
        assert!(
            !self.identity.ip.is_empty() && !self.identity.access_code.is_empty(),
            "with_ftps() requires a real ip/access_code — this PrinterClient was built via \
             from_mqtt(), which leaves both empty; use .attach_storage() instead"
        );
        PrinterClient {
            mqtt: self.mqtt,
            ftps: None,
            ftps_config: Some((tls, factory, timer)),
            camera: self.camera,
            camera_config: self.camera_config,
            mqtt_tls: self.mqtt_tls,
            mqtt_factory: self.mqtt_factory,
            timer: self.timer,
            identity: self.identity,
            sequence_counter: self.sequence_counter,
            k_profile_primed: self.k_profile_primed,
            cache: self.cache,
            command_timeout_secs: self.command_timeout_secs,
            connect_timeout_secs: self.connect_timeout_secs,
            mqtt_port: self.mqtt_port,
            ftps_port: self.ftps_port,
            ftps_allow_unverified_tls_1_2: self.ftps_allow_unverified_tls_1_2,
            camera_port: self.camera_port,
            camera_max_frame_size: self.camera_max_frame_size,
            _mqtt_raw_io: PhantomData,
            _camera_raw_io: PhantomData,
        }
    }

    /// Overrides the default FTPS port (990).
    #[must_use]
    pub fn with_ftps_port(mut self, port: u16) -> Self {
        self.ftps_port = port;
        self
    }

    /// Overrides the default `false` for `FtpsClient`'s TLS-1.2-enforcement bypass.
    ///
    /// Only meaningful for the `embassy` feature talking to P2S/X2D, where no available TLS
    /// backend can honestly satisfy `require_tls_1_2_if_enforced`'s exact-version check —
    /// see `src/ftps/CLAUDE.md` and `src/io/CLAUDE.md`. On `tokio`/`esp-idf`, use
    /// `force_tls_1_2` on the `TlsConnector` instead, since those platforms can actually
    /// satisfy the check for real.
    /// Non-consuming — chain onto any construction path.
    #[must_use]
    pub fn with_ftps_allow_unverified_tls_1_2(mut self, allow: bool) -> Self {
        self.ftps_allow_unverified_tls_1_2 = allow;
        self
    }

    /// Establishes the FTPS connection if not already connected.
    ///
    /// Short-circuits when `self.ftps` is already `Some`. Otherwise, borrows the TLS
    /// connector and data factory from `ftps_config`, dials a raw connection, and runs
    /// `FtpsClient::connect_control_stream()` — the whole dial+connect sequence is
    /// raced against `self.connect_timeout_secs`. `ftps_config` is only consumed
    /// (`.take()`n) once that attempt has actually succeeded — a failed attempt,
    /// including a `connect_timeout_secs` timeout on a slow LAN, leaves it intact so the
    /// next call retries instead of permanently reporting "not configured". Reconnecting
    /// after a *successful* connect still requires a new `PrinterClient`.
    pub(super) async fn ensure_ftps(&mut self) -> Result<(), Error> {
        if self.ftps.is_some() {
            return Ok(());
        }
        let (tls, factory, timer) = self.ftps_config.as_ref().ok_or_else(|| {
            Error::ProtocolViolation(
                "FTPS not configured — call .with_ftps() or .attach_storage()".into(),
            )
        })?;
        let identity = &self.identity;
        let ftps_port = self.ftps_port;
        let allow_unverified_tls_1_2 = self.ftps_allow_unverified_tls_1_2;
        let (control_stream, fill_buf) =
            race_against_connect_timeout(&self.timer, self.connect_timeout_secs, async {
                let raw_stream = factory.dial(&identity.ip, ftps_port).await?;
                FtpsClient::<FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer>::connect_control_stream(
                    raw_stream,
                    tls,
                    identity,
                    timer,
                    allow_unverified_tls_1_2,
                )
                .await
            })
            .await?;
        // Safe to consume now — the handshake above already succeeded.
        let (tls, factory, timer) = self.ftps_config.take().unwrap();
        self.ftps = Some(FtpsClient::from_control_stream(
            control_stream,
            tls,
            factory,
            &self.identity,
            timer,
            allow_unverified_tls_1_2,
            fill_buf,
        ));
        Ok(())
    }

    /// Eagerly establishes the FTPS connection.
    ///
    /// Idempotent — returns `Ok(())` if already connected.
    pub async fn connect_ftps(&mut self) -> Result<(), Error> {
        self.ensure_ftps().await
    }

    /// Returns whether the FTPS session is currently established.
    pub fn is_ftps_connected(&self) -> bool {
        self.ftps.is_some()
    }

    /// Establishes the camera connection if not already connected.
    ///
    /// Returns `Error::ProtocolViolation` immediately for RTSPS models — those use
    /// `camera::rtsps::build_rtsps_url()` instead and have no `PrinterClient`-managed
    /// connection state. Otherwise dials a raw stream via the camera factory, wraps it in
    /// TLS, constructs a `BinaryCameraStream`, and authenticates — the whole sequence is
    /// raced against `self.connect_timeout_secs`, mirroring `ensure_ftps()`.
    pub(super) async fn ensure_camera(&mut self) -> Result<(), Error> {
        if self.identity.model.quirks().camera_protocol() != CameraProtocol::BinaryJpeg {
            return Err(Error::ProtocolViolation(
                "This model uses RTSPS for its camera feed — use camera::rtsps::build_rtsps_url() instead"
                    .into(),
            ));
        }
        if self.camera.is_some() {
            return Ok(());
        }
        let (tls, factory) = self.camera_config.as_ref().ok_or_else(|| {
            Error::ProtocolViolation(
                "Camera not configured — call .with_camera() or .attach_camera()".into(),
            )
        })?;
        let identity = &self.identity;
        let ip = &identity.ip;
        let serial = &identity.serial;
        let camera_port = self.camera_port;
        let max_frame_size = self.camera_max_frame_size;
        let camera_stream =
            race_against_connect_timeout(&self.timer, self.connect_timeout_secs, async {
                let raw = factory.dial(ip, camera_port).await?;
                let stream = tls.connect(serial, raw).await?;
                let mut cam = BinaryCameraStream::new(stream);
                if let Some(max) = max_frame_size {
                    cam = cam.with_max_frame_size(max);
                }
                cam.authenticate(identity).await?;
                Ok::<_, Error>(cam)
            })
            .await?;
        // Only clear camera_config once the connection has actually succeeded — a failed
        // attempt (including a connect_timeout_secs timeout on a slow LAN) must leave it
        // intact so the next call retries instead of permanently reporting "not configured".
        self.camera_config = None;
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
    ///   [`Error::ProtocolViolation`] on an RTSPS model: here an RTSPS camera is a channel
    ///   that does not apply to this printer, not a failure, so reporting it as an error
    ///   would hand every P2S/X2D consumer a guaranteed `Err` on an otherwise clean connect.
    ///   Those models use `camera::rtsps::build_rtsps_url()` and have no client-managed
    ///   connection to establish.
    ///
    /// # Timeouts
    ///
    /// `connect_timeout_secs` is applied **per channel**, matching the individual
    /// `ensure_*` methods, so a slow or unreachable camera can never cause an otherwise
    /// healthy MQTT dial to be reported as timed out. Because the channels run concurrently
    /// the worst-case wall clock for the whole call is still one timeout, not three. A
    /// shared deadline around the joined future was rejected precisely because it cannot
    /// express partial success: it would discard an already-completed MQTT session when a
    /// hung camera pushed the *combined* future past the deadline.
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
        let secs = self.connect_timeout_secs;
        let identity = &self.identity;

        let mqtt_wanted = self.mqtt.is_none();
        let mqtt_factory = &self.mqtt_factory;
        let mqtt_tls = &self.mqtt_tls;
        let mqtt_port = self.mqtt_port;
        let mqtt_fut = async move {
            if !mqtt_wanted {
                return None;
            }
            Some(
                race_against_connect_timeout(timer, secs, async {
                    let raw = mqtt_factory.dial(&identity.ip, mqtt_port).await?;
                    let stream = mqtt_tls.connect(&identity.serial, raw).await?;
                    MqttClient::connect(stream, identity).await
                })
                .await,
            )
        };

        // `as_ref()` only — `ftps_config` is consumed after the join, and only on success,
        // so a failed attempt still leaves it available for a retry.
        let ftps_slot = if self.ftps.is_none() {
            self.ftps_config.as_ref()
        } else {
            None
        };
        let ftps_port = self.ftps_port;
        let allow_unverified_tls_1_2 = self.ftps_allow_unverified_tls_1_2;
        let ftps_fut = async move {
            let (tls, factory, ftps_timer) = ftps_slot?;
            Some(
                race_against_connect_timeout(timer, secs, async {
                    let raw_stream = factory.dial(&identity.ip, ftps_port).await?;
                    FtpsClient::<FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer>::connect_control_stream(
                        raw_stream,
                        tls,
                        identity,
                        ftps_timer,
                        allow_unverified_tls_1_2,
                    )
                    .await
                })
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
        let camera_port = self.camera_port;
        let max_frame_size = self.camera_max_frame_size;
        let camera_fut = async move {
            let (tls, factory) = camera_slot?;
            Some(
                race_against_connect_timeout(timer, secs, async {
                    let raw = factory.dial(&identity.ip, camera_port).await?;
                    let stream = tls.connect(&identity.serial, raw).await?;
                    let mut cam = BinaryCameraStream::new(stream);
                    if let Some(max) = max_frame_size {
                        cam = cam.with_max_frame_size(max);
                    }
                    cam.authenticate(identity).await?;
                    Ok::<_, Error>(cam)
                })
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
                self.mqtt = Some(client);
                // Same wall-clock reseed `ensure_mqtt()` performs, and for the same reason —
                // see its comment on colliding sequence IDs between independent sessions.
                if self.timer.has_real_clock() {
                    self.sequence_counter =
                        crate::mqtt::commands::clamp_task_id(self.timer.now_millis()) as u64;
                }
                Some(Ok(()))
            }
        };

        let ftps = match ftps_res {
            None => None,
            Some(Err(e)) => Some(Err(e)),
            Some(Ok((control_stream, fill_buf))) => {
                // Safe to consume now — the handshake above already succeeded, and
                // `ftps_res` is only `Some` when `ftps_slot` was.
                let (tls, factory, ftps_timer) = self.ftps_config.take().unwrap();
                self.ftps = Some(FtpsClient::from_control_stream(
                    control_stream,
                    tls,
                    factory,
                    &self.identity,
                    ftps_timer,
                    allow_unverified_tls_1_2,
                    fill_buf,
                ));
                Some(Ok(()))
            }
        };

        let camera = match camera_res {
            None => None,
            Some(Err(e)) => Some(Err(e)),
            Some(Ok(cam)) => {
                self.camera_config = None;
                self.camera = Some(cam);
                Some(Ok(()))
            }
        };

        ConnectAllOutcome { mqtt, ftps, camera }
    }

    /// Configures the binary-JPEG camera for lazy connection on first camera method call.
    ///
    /// Consuming builder — changes the `CameraRawIO`, `CameraTls`, and `CameraFactory` type
    /// parameters. Independent of MQTT's and FTPS's connectors, mirroring `.with_ftps()`.
    ///
    /// Must not be called on a client with an already-connected camera session — see
    /// `.with_ftps()`'s doc comment for why.
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
        // Same guard as with_ftps() — from_mqtt()-constructed clients have empty
        // ip/access_code, which would otherwise fail opaquely at actual camera connect time.
        assert!(
            !self.identity.ip.is_empty() && !self.identity.access_code.is_empty(),
            "with_camera() requires a real ip/access_code — this PrinterClient was built via \
             from_mqtt(), which leaves both empty; use .attach_camera() instead"
        );
        PrinterClient {
            mqtt: self.mqtt,
            ftps: self.ftps,
            ftps_config: self.ftps_config,
            camera: None,
            camera_config: Some((tls, factory)),
            mqtt_tls: self.mqtt_tls,
            mqtt_factory: self.mqtt_factory,
            timer: self.timer,
            identity: self.identity,
            sequence_counter: self.sequence_counter,
            k_profile_primed: self.k_profile_primed,
            cache: self.cache,
            command_timeout_secs: self.command_timeout_secs,
            connect_timeout_secs: self.connect_timeout_secs,
            mqtt_port: self.mqtt_port,
            ftps_port: self.ftps_port,
            ftps_allow_unverified_tls_1_2: self.ftps_allow_unverified_tls_1_2,
            camera_port: self.camera_port,
            camera_max_frame_size: self.camera_max_frame_size,
            _mqtt_raw_io: PhantomData,
            _camera_raw_io: PhantomData,
        }
    }

    /// Overrides the default camera port (6000, binary-JPEG only).
    #[must_use]
    pub fn with_camera_port(mut self, port: u16) -> Self {
        self.camera_port = port;
        self
    }

    /// Overrides the default maximum accepted camera frame size (see `BinaryCameraStream::with_max_frame_size`).
    #[must_use]
    pub fn with_camera_max_frame_size(mut self, bytes: usize) -> Self {
        self.camera_max_frame_size = Some(bytes);
        self
    }
}
