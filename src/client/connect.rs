use core::future::Future;
use core::marker::PhantomData;

use crate::camera::CameraProtocol;
use crate::camera::binary::BambuBinaryCameraStream;
use crate::error::Error;
use crate::ftps::BambuFtpsClient;
use crate::io::{AsyncIo, Raced, RawStreamFactory, SocketError, TimerProvider, TlsConnector, race};
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
                let raw = self.mqtt_factory.dial(&self.identity.ip, self.mqtt_port).await?;
                let stream = self.mqtt_tls.connect(&self.identity.serial, raw).await?;
                MqttClient::connect(stream, &self.identity.serial, &self.identity.access_code).await
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
            model: self.model,
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
    /// constructed fresh by the caller (e.g. `TokioTimer::new()`) — `BambuFtpsClient` owns it
    /// independently of `PrinterClient`'s own `Timer`, since `PrinterClient::storage()` hands
    /// out direct `&mut BambuFtpsClient` access rather than mediating every FTPS call itself,
    /// so there's no call site to thread `self.timer` through the way MQTT/camera do.
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
        // BUG-072: `from_mqtt()`-constructed clients have empty `ip`/`access_code` (no host
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
            model: self.model,
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

    /// Overrides the default `false` for `BambuFtpsClient`'s TLS-1.2-enforcement bypass.
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
    /// `BambuFtpsClient::connect_control_stream()` — the whole dial+connect sequence is
    /// raced against `self.connect_timeout_secs`. `ftps_config` is only consumed
    /// (`.take()`n) once that attempt has actually succeeded (BUG-020) — a failed attempt,
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
        let ip = &self.identity.ip;
        let serial = &self.identity.serial;
        let access_code = &self.identity.access_code;
        let model = self.model;
        let ftps_port = self.ftps_port;
        let allow_unverified_tls_1_2 = self.ftps_allow_unverified_tls_1_2;
        let (control_stream, fill_buf) =
            race_against_connect_timeout(&self.timer, self.connect_timeout_secs, async {
                let raw_stream = factory.dial(ip, ftps_port).await?;
                BambuFtpsClient::<FtpsRawIO, FtpsTls, FtpsFactory, FtpsTimer>::connect_control_stream(
                    raw_stream,
                    tls,
                    model,
                    serial,
                    access_code,
                    timer,
                    allow_unverified_tls_1_2,
                )
                .await
            })
            .await?;
        // Safe to consume now — the handshake above already succeeded.
        let (tls, factory, timer) = self.ftps_config.take().unwrap();
        self.ftps = Some(BambuFtpsClient::from_control_stream(
            control_stream,
            tls,
            factory,
            model,
            ip,
            serial,
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
    /// TLS, constructs a `BambuBinaryCameraStream`, and authenticates — the whole sequence is
    /// raced against `self.connect_timeout_secs`, mirroring `ensure_ftps()`.
    pub(super) async fn ensure_camera(&mut self) -> Result<(), Error> {
        if self.model.quirks().camera_protocol() != CameraProtocol::BinaryJpeg {
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
        let ip = &self.identity.ip;
        let serial = &self.identity.serial;
        let access_code = &self.identity.access_code;
        let camera_port = self.camera_port;
        let max_frame_size = self.camera_max_frame_size;
        let camera_stream =
            race_against_connect_timeout(&self.timer, self.connect_timeout_secs, async {
                let raw = factory.dial(ip, camera_port).await?;
                let stream = tls.connect(serial, raw).await?;
                let mut cam = BambuBinaryCameraStream::new(stream);
                if let Some(max) = max_frame_size {
                    cam = cam.with_max_frame_size(max);
                }
                cam.authenticate(access_code).await?;
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

    /// Configures the binary-JPEG camera for lazy connection on first camera method call.
    ///
    /// Consuming builder — changes the `CameraRawIO`, `CameraTls`, and `CameraFactory` type
    /// parameters. Independent of MQTT's and FTPS's connectors, mirroring `.with_ftps()`.
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
        // BUG-072: same guard as with_ftps() — from_mqtt()-constructed clients have empty
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
            model: self.model,
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

    /// Overrides the default maximum accepted camera frame size (see `BambuBinaryCameraStream::with_max_frame_size`).
    #[must_use]
    pub fn with_camera_max_frame_size(mut self, bytes: usize) -> Self {
        self.camera_max_frame_size = Some(bytes);
        self
    }
}
