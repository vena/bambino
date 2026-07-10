use core::future::Future;
use core::marker::PhantomData;

use crate::camera::CameraProtocol;
use crate::camera::binary::BambuBinaryCameraStream;
use crate::error::BambuError;
use crate::ftps::BambuFtpsClient;
use crate::io::{AsyncIo, Raced, RawStreamFactory, SocketError, TimerProvider, TlsConnector, race};
use crate::mqtt::BambuMqttClient;

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
    /// `BambuMqttClient::connect()` — the whole dial+TLS+handshake sequence is raced against
    /// `self.connect_timeout_secs`.
    pub(super) async fn ensure_mqtt(&mut self) -> Result<(), BambuError> {
        if self.mqtt.is_some() {
            return Ok(());
        }
        let mqtt_client =
            race_against_connect_timeout(&self.timer, self.connect_timeout_secs, async {
                let raw = self.mqtt_factory.dial(&self.ip, self.mqtt_port).await?;
                let stream = self.mqtt_tls.connect(&self.serial, raw).await?;
                BambuMqttClient::connect(stream, &self.serial, &self.access_code).await
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
    pub async fn connect_mqtt(&mut self) -> Result<(), BambuError> {
        self.ensure_mqtt().await
    }

    /// Returns whether the MQTT session is currently established.
    pub fn mqtt_connected(&self) -> bool {
        self.mqtt.is_some()
    }

    /// Injects a pre-connected [`BambuMqttClient`] directly.
    ///
    /// Use this for test mocks or Embassy where the caller manages the MQTT connection,
    /// mirroring [`attach_camera()`](super::PrinterClient::attach_camera)/
    /// [`attach_storage()`](super::PrinterClient::attach_storage).
    pub fn attach_mqtt(&mut self, mqtt: BambuMqttClient<MqttTls::Stream>) {
        self.mqtt = Some(mqtt);
    }

    /// Disconnects the MQTT session, if one exists, and clears it from the client.
    ///
    /// There is no protocol-level teardown on `BambuMqttClient` to call — this just clears
    /// the slot, mirroring `disconnect_camera()`. Without this, a dead stream (a
    /// [`tick_zombie_check()`](crate::mqtt::BambuMqttClient::tick_zombie_check)-detected
    /// zombie, a transport error) left `self.mqtt` stuck `Some(...)` forever, since
    /// `ensure_mqtt()`'s `is_some()` short-circuit kept handing back the same broken
    /// connection with no supported redial path.
    ///
    /// Idempotent. Reconnecting requires either [`.attach_mqtt()`](Self::attach_mqtt) with a
    /// fresh `BambuMqttClient`, or letting the next call fall through to `ensure_mqtt()`'s
    /// own lazy dial.
    pub async fn disconnect_mqtt(&mut self) -> Result<(), BambuError> {
        self.mqtt = None;
        Ok(())
    }

    /// Sets a [`TimerProvider`] for wall-clock command-response timeouts.
    ///
    /// Consuming builder — works on both [`new()`](PrinterClient::new) and
    /// [`from_mqtt()`](PrinterClient::from_mqtt) construction paths.
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
            serial: self.serial,
            ip: self.ip,
            access_code: self.access_code,
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
    pub fn with_mqtt_port(mut self, port: u16) -> Self {
        self.mqtt_port = port;
        self
    }

    /// Overrides the default connect-timeout deadline (10s) that bounds `ensure_mqtt()`/`ensure_ftps()`'s combined dial+TLS-connect sequence.
    /// Passing `0` disables the timeout entirely, matching `set_command_timeout`'s "0 disables"
    /// convention. Non-consuming — chain onto any construction path.
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
            serial: self.serial,
            ip: self.ip,
            access_code: self.access_code,
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
    pub fn with_ftps_allow_unverified_tls_1_2(mut self, allow: bool) -> Self {
        self.ftps_allow_unverified_tls_1_2 = allow;
        self
    }

    /// Establishes the FTPS connection if not already connected.
    ///
    /// Short-circuits when `self.ftps` is already `Some`. Otherwise, takes the TLS connector
    /// and data factory from `ftps_config`, dials a raw connection, and calls
    /// `BambuFtpsClient::connect()` — the whole dial+connect sequence is raced against
    /// `self.connect_timeout_secs`. The config is consumed on first connection —
    /// reconnecting requires a new `PrinterClient`.
    pub(super) async fn ensure_ftps(&mut self) -> Result<(), BambuError> {
        if self.ftps.is_some() {
            return Ok(());
        }
        let (tls, factory, timer) = self.ftps_config.take().ok_or_else(|| {
            BambuError::ProtocolViolation(
                "FTPS not configured — call .with_ftps() or .attach_storage()".into(),
            )
        })?;
        let ip = &self.ip;
        let serial = &self.serial;
        let access_code = &self.access_code;
        let model = self.model;
        let ftps_port = self.ftps_port;
        let allow_unverified_tls_1_2 = self.ftps_allow_unverified_tls_1_2;
        let ftps_client =
            race_against_connect_timeout(&self.timer, self.connect_timeout_secs, async move {
                let raw_stream = factory.dial(ip, ftps_port).await?;
                BambuFtpsClient::connect(
                    raw_stream,
                    tls,
                    factory,
                    model,
                    ip,
                    serial,
                    access_code,
                    timer,
                    allow_unverified_tls_1_2,
                )
                .await
            })
            .await?;
        self.ftps = Some(ftps_client);
        Ok(())
    }

    /// Eagerly establishes the FTPS connection.
    ///
    /// Idempotent — returns `Ok(())` if already connected.
    pub async fn connect_ftps(&mut self) -> Result<(), BambuError> {
        self.ensure_ftps().await
    }

    /// Returns whether the FTPS session is currently established.
    pub fn ftps_connected(&self) -> bool {
        self.ftps.is_some()
    }

    /// Establishes the camera connection if not already connected.
    ///
    /// Returns `BambuError::ProtocolViolation` immediately for RTSPS models — those use
    /// `camera::rtsps::build_rtsps_url()` instead and have no `PrinterClient`-managed
    /// connection state. Otherwise dials a raw stream via the camera factory, wraps it in
    /// TLS, constructs a `BambuBinaryCameraStream`, and authenticates — the whole sequence is
    /// raced against `self.connect_timeout_secs`, mirroring `ensure_ftps()`.
    pub(super) async fn ensure_camera(&mut self) -> Result<(), BambuError> {
        if self.model.quirks().camera_protocol() != CameraProtocol::BinaryJpeg {
            return Err(BambuError::ProtocolViolation(
                "This model uses RTSPS for its camera feed — use camera::rtsps::build_rtsps_url() instead"
                    .into(),
            ));
        }
        if self.camera.is_some() {
            return Ok(());
        }
        let (tls, factory) = self.camera_config.take().ok_or_else(|| {
            BambuError::ProtocolViolation(
                "Camera not configured — call .with_camera() or .attach_camera()".into(),
            )
        })?;
        let ip = &self.ip;
        let serial = &self.serial;
        let access_code = &self.access_code;
        let camera_port = self.camera_port;
        let max_frame_size = self.camera_max_frame_size;
        let camera_stream =
            race_against_connect_timeout(&self.timer, self.connect_timeout_secs, async move {
                let raw = factory.dial(ip, camera_port).await?;
                let stream = tls.connect(serial, raw).await?;
                let mut cam = BambuBinaryCameraStream::new(stream);
                if let Some(max) = max_frame_size {
                    cam = cam.with_max_frame_size(max);
                }
                cam.authenticate(access_code).await?;
                Ok::<_, BambuError>(cam)
            })
            .await?;
        self.camera = Some(camera_stream);
        Ok(())
    }

    /// Eagerly establishes the camera connection.
    ///
    /// Idempotent — returns `Ok(())` if already connected.
    pub async fn connect_camera(&mut self) -> Result<(), BambuError> {
        self.ensure_camera().await
    }

    /// Returns whether the camera session is currently established.
    pub fn camera_connected(&self) -> bool {
        self.camera.is_some()
    }

    /// Configures the binary-JPEG camera for lazy connection on first camera method call.
    ///
    /// Consuming builder — changes the `CameraRawIO`, `CameraTls`, and `CameraFactory` type
    /// parameters. Independent of MQTT's and FTPS's connectors, mirroring `.with_ftps()`.
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
            serial: self.serial,
            ip: self.ip,
            access_code: self.access_code,
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
    pub fn with_camera_port(mut self, port: u16) -> Self {
        self.camera_port = port;
        self
    }

    /// Overrides the default maximum accepted camera frame size (see `BambuBinaryCameraStream::with_max_frame_size`).
    pub fn with_camera_max_frame_size(mut self, bytes: usize) -> Self {
        self.camera_max_frame_size = Some(bytes);
        self
    }
}
