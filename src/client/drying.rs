//! # Drying Cycle Builder
//!
//! [`DryingCycle`] starts an AMS drying cycle without a nine-argument positional call.
//!
//! The wire command carries eight parameters, four of which most callers want defaulted
//! (`humidity`, `rotate_tray`, `cooling_temp`, `close_power_conflict`), and two of which are
//! adjacent `bool`s that nothing in the type system keeps in order. The builder names each one
//! at the call site and defaults the rest to what BambuStudio itself sends.
//!
//! It also gives [`DryingMaterial`] a seam. The vendor publishes a temperature, a duration and a
//! cooling temperature per material, and on a positional call a caller has to thread those into
//! three of nine slots by hand; [`material`](DryingCycle::material) sets all three from one
//! choice.

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use crate::ams::parser::{AMS_LITE_ON_A2L_NORMALIZED_ID, AMS_LITE_ON_A2L_PHYSICAL_ID};
use crate::error::Error;
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};
use crate::mqtt::{AmsFilamentDryingRequest, DryingParams};
use crate::types::DryingMaterial;
use crate::types::drying::DEFAULT_COMMAND_COOLING_TEMP;
use crate::types::telemetry::AmsUnitModel;

use super::ams::{is_ams_ht_id, is_valid_ams_id, wire_ams_id};
use super::{CommandHandle, PrinterClient};

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
    /// Configures a drying cycle for the unit at `ams_id`, to be sent with
    /// [`send()`](crate::client::DryingCycle::send).
    ///
    /// The way to start drying. Names each parameter at the call site instead of ordering nine
    /// of them, defaults the four most callers don't set, and lets
    /// [`material()`](crate::client::DryingCycle::material) fill temperature, duration and
    /// cooling temperature from one choice:
    ///
    /// ```rust,ignore
    /// client
    ///     .dry(0)
    ///     .material(DryingMaterial::Petg, AmsUnitModel::Ams2Pro)
    ///     .rotate_tray(true)
    ///     .send()
    ///     .await?;
    /// ```
    ///
    /// Nothing is published until [`send()`](crate::client::DryingCycle::send), which is where
    /// every gate runs — host capability, AMS addressing, the external-spool sentinels, the
    /// attached unit's model, and the temperature range [REF-AMS-DRYER].
    pub fn dry(
        &mut self,
        ams_id: i32,
    ) -> crate::client::DryingCycle<
        '_,
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
    > {
        crate::client::DryingCycle::new(self, ams_id)
    }

    /// Terminates an active dry-chamber heating cycle on an AMS unit [REF-AMS-DRYER].
    ///
    /// Mirrors BambuStudio's `CtrlAmsStopDrying` (`DevFilaSystemCtrl.cpp:40-53`) exactly —
    /// every field zeroed/defaulted, only `mode: 0` (`Off`) is meaningful.
    pub async fn stop_drying(&mut self, ams_id: i32) -> Result<CommandHandle, Error> {
        if !is_valid_ams_id(ams_id) {
            return Err(Error::ProtocolViolation(
                "invalid AMS addressing parameters for stop_drying".into(),
            ));
        }
        let ams_id = wire_ams_id(ams_id);
        self.dispatch(|seq| AmsFilamentDryingRequest::stop(ams_id, seq))
            .await
    }
}

/// A drying cycle being configured, returned by [`PrinterClient::dry`].
///
/// Nothing is sent until [`send()`](Self::send), which runs the same validation the command
/// always did — host capability, unit model, temperature range — and publishes.
///
/// ```rust,ignore
/// client
///     .dry(0)
///     .material(DryingMaterial::Petg, AmsUnitModel::Ams2Pro)
///     .rotate_tray(true)
///     .send()
///     .await?;
/// ```
#[must_use = "a drying cycle does nothing until .send() is awaited"]
pub struct DryingCycle<
    'a,
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
> where
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
    client: &'a mut PrinterClient<
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
    >,
    ams_id: i32,
    /// Vendor parameters to fill whatever wasn't set explicitly, with the while-printing flag.
    material: Option<(DryingMaterial, AmsUnitModel)>,
    printing: bool,
    temp: Option<u32>,
    duration_hours: Option<u32>,
    cooling_temp: Option<u32>,
    filament: Option<String>,
    humidity: u32,
    rotate_tray: bool,
    close_power_conflict: bool,
}

impl<
    'a,
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
    DryingCycle<
        'a,
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
    /// Starts a cycle for the unit at `ams_id`, with vendor defaults for everything optional.
    ///
    /// `temp` and `duration_hours` have no default, and [`send()`](Self::send) rejects a cycle
    /// without them — silently picking one would start a heating cycle nobody asked for. Set
    /// them with [`material()`](Self::material) or explicitly.
    pub(crate) fn new(
        client: &'a mut PrinterClient<
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
        >,
        ams_id: i32,
    ) -> Self {
        Self {
            client,
            ams_id,
            material: None,
            printing: false,
            temp: None,
            duration_hours: None,
            cooling_temp: None,
            filament: None,
            humidity: 0,
            rotate_tray: false,
            close_power_conflict: false,
        }
    }

    /// Uses the vendor's published parameters for `material` on `unit` for temperature,
    /// duration, cooling temperature and the filament name.
    ///
    /// These are defaults, resolved in [`send()`](Self::send): an explicit
    /// [`temp()`](Self::temp), [`duration_hours()`](Self::duration_hours),
    /// [`cooling_temp()`](Self::cooling_temp) or [`filament()`](Self::filament) wins regardless
    /// of call order. Calling this again replaces the material.
    ///
    /// The cooling temperature sent is the material's
    /// [`softening_temp`](DryingMaterial::softening_temp), which is what the wire field carries
    /// (see its doc). A unit without a drying chamber has no published parameters, so temperature
    /// and duration stay unset and [`send()`](Self::send) rejects rather than publishing a guess.
    pub fn material(mut self, material: DryingMaterial, unit: AmsUnitModel) -> Self {
        self.material = Some((material, unit));
        self
    }

    /// Reads the material's defaults from the lower while-printing column, which exists because
    /// the AMS sits in the print's thermal envelope.
    ///
    /// Affects only defaults from [`material()`](Self::material), in either call order; explicit
    /// values are sent as set.
    pub fn printing(mut self) -> Self {
        self.printing = true;
        self
    }

    /// Sets the drying temperature in °C, overriding any material default.
    pub fn temp(mut self, temp: u32) -> Self {
        self.temp = Some(temp);
        self
    }

    /// Sets the cycle duration in whole hours, overriding any material default.
    pub fn duration_hours(mut self, hours: u32) -> Self {
        self.duration_hours = Some(hours);
        self
    }

    /// Sets the filament type string sent as the wire `dry_filament` field.
    ///
    /// Free-form by design — the wire field is arbitrary text and BambuStudio sends the tray's
    /// own `filament_type`. Use this for a material [`DryingMaterial`] does not name.
    pub fn filament(mut self, filament: &str) -> Self {
        self.filament = Some(filament.to_string());
        self
    }

    /// Sets the target humidity. `0`, the default, means "firmware default / no target".
    pub fn humidity(mut self, humidity: u32) -> Self {
        self.humidity = humidity;
        self
    }

    /// Whether to rotate trays during the cycle. Defaults to `false`.
    pub fn rotate_tray(mut self, rotate: bool) -> Self {
        self.rotate_tray = rotate;
        self
    }

    /// Sets the cooling temperature sent with the command.
    ///
    /// Defaults to the [`material()`](Self::material)'s softening temperature, else
    /// [`DEFAULT_COMMAND_COOLING_TEMP`], BambuStudio's own fallback.
    pub fn cooling_temp(mut self, cooling_temp: u32) -> Self {
        self.cooling_temp = Some(cooling_temp);
        self
    }

    /// Whether to override the AMS unit's power-conflict interlock. Defaults to `false`.
    ///
    /// The interlock exists because several drying units on one supply can exceed it; overriding
    /// it is the caller asserting they know the power situation.
    pub fn close_power_conflict(mut self, close: bool) -> Self {
        self.close_power_conflict = close;
        self
    }

    /// Validates and publishes the cycle, returning the published command's [`CommandHandle`] [REF-AMS-DRYER].
    ///
    /// Every gate lives here — this is the only path that publishes `ams_filament_drying`, so it
    /// is the only place a future check has to be added.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidArgument`] when no temperature or duration was set. These deliberately
    /// have no default: silently picking one would start a real heating cycle the caller never
    /// asked for. Set them with [`material()`](Self::material) or explicitly.
    ///
    /// [`Error::ModelMismatch`] on a host where
    /// [`supports_ams_remote_drying()`](PrinterClient::supports_ams_remote_drying) is `false` —
    /// the printer's own `fun2` bit 5 where it reported one, else the model's rule: never on
    /// A1/A1 Mini, P1P/P1S or X1/X1C, and below the minimum firmware on
    /// H2D/H2D Pro/H2S/H2C/P2S/X2D. Such
    /// firmware acks this command `result: success` and silently discards it rather than driving
    /// the AMS heater.
    ///
    /// [`Error::ModelMismatch`] also when the addressed unit has no drying chamber — an
    /// external-spool sentinel (`254`/`255`), an AMS Lite on an A2L (`6`/`16`, an id only that
    /// heaterless unit takes), or a cached [`AmsUnitModel`] whose
    /// [`supports_drying`](AmsUnitModel::supports_drying) is `false`.
    /// These are two independent gates on purpose, matching the pair BambuStudio writes out
    /// longhand at `Widgets/AMSControl.cpp:348`: the printer must act on the command *and* the
    /// attached box must have a heater.
    ///
    /// [`Error::ProtocolViolation`] for an `ams_id` outside the documented address space.
    ///
    /// [`Error::InvalidArgument`] when the temperature falls outside the unit's
    /// [`dry_temp_range`](AmsUnitModel::dry_temp_range). **Both bounds are rejected, not
    /// clamped**: BambuStudio refuses a temperature below the floor exactly as it refuses one
    /// above the ceiling (`AMSDryControl.cpp:1186-1199`), and silently rewriting a caller's value
    /// would start a heating cycle they did not ask for.
    ///
    /// The unit-model gate reads the **cached** AMS snapshot, so a unit this client has never
    /// observed passes through — the rule [`skip_objects`](PrinterClient::skip_objects)
    /// established, and for the same reason: an idle printer's incremental pushes frequently
    /// carry no `ams` block at all, and refusing there would break a caller that connects and
    /// commands without polling. Call [`poll_telemetry()`](PrinterClient::poll_telemetry) first
    /// to arm it. When the unit is unobserved the temperature range falls back to the
    /// `ams_id`-derived ceiling, the best guess the address alone supports.
    ///
    /// A temperature above the filament's heat-distortion temperature
    /// ([`DryingMaterial::heat_distortion_temp`], for a filament string
    /// [`DryingMaterial::from_filament_type`] recognizes) is sent as asked but logged with
    /// `log::warn!`. BambuStudio refuses such a cycle on a loaded tray; here an explicit
    /// [`temp()`](Self::temp) is the caller's call, and [`material()`](Self::material) never
    /// picks one.
    pub async fn send(self) -> Result<CommandHandle, Error> {
        let material = self.material.map(|(material, _)| material);
        let default = |pick: fn(DryingMaterial, AmsUnitModel, bool) -> Option<u32>| {
            self.material
                .and_then(|(material, unit)| pick(material, unit, self.printing))
        };
        let temp = self
            .temp
            .or_else(|| default(DryingMaterial::default_temp))
            .filter(|&t| t != 0)
            .ok_or_else(|| {
                Error::InvalidArgument(
                    "drying temperature not set — call .material(..) or .temp(..)".into(),
                )
            })?;
        let duration_hours = self
            .duration_hours
            .or_else(|| default(DryingMaterial::default_duration_hours))
            .filter(|&h| h != 0)
            .ok_or_else(|| {
                Error::InvalidArgument(
                    "drying duration not set — call .material(..) or .duration_hours(..)".into(),
                )
            })?;
        let cooling_temp = self
            .cooling_temp
            .or(material.map(DryingMaterial::softening_temp))
            // BambuStudio's own fallback when a tray's filament resolves to no preset
            // (`AMSDryControl.cpp:813`), not a zero.
            .unwrap_or(DEFAULT_COMMAND_COOLING_TEMP);
        let filament = self
            .filament
            .or_else(|| material.map(|m| m.wire_name().to_string()))
            .unwrap_or_default();

        if !self.client.supports_ams_remote_drying() {
            return Err(Error::ModelMismatch(
                "AMS drying is screen-only on this host printer — firmware acks this command but does not act on it".into(),
            ));
        }
        if !is_valid_ams_id(self.ams_id) {
            return Err(Error::ProtocolViolation(
                "invalid AMS addressing parameters for a drying cycle".into(),
            ));
        }
        // An external spool is a holder on a bracket, not a box with a heater — the one place
        // the address *does* settle the capability, since 254/255 never appear in the `ams`
        // array for the cached lookup below to find.
        if self.ams_id == 254 || self.ams_id == 255 {
            return Err(Error::ModelMismatch(
                "external spool has no drying chamber — drying needs an AMS 2 Pro or AMS-HT".into(),
            ));
        }
        // Same for an AMS Lite on an A2L: `6`/`16` is never any other unit, so the address alone
        // settles it before telemetry has named the unit.
        if self.ams_id == i32::from(AMS_LITE_ON_A2L_NORMALIZED_ID)
            || self.ams_id == i32::from(AMS_LITE_ON_A2L_PHYSICAL_ID)
        {
            return Err(Error::ModelMismatch(
                "AMS Lite has no drying chamber — drying needs an AMS 2 Pro or AMS-HT".into(),
            ));
        }

        let unit_model = self.client.ams_unit_model(self.ams_id);
        if let Some(model) = unit_model
            && !model.supports_drying()
        {
            return Err(Error::ModelMismatch(
                "attached AMS unit has no drying chamber — only the AMS 2 Pro and AMS-HT can dry"
                    .into(),
            ));
        }

        // `dry_temp_range()` is the authority when the unit is known. Unobserved, assume the
        // dryer the address implies — wrong for an original AMS or an AMS Lite at `0..=3`, but
        // that is exactly the case the gate above cannot rule on either.
        let assumed_model = unit_model.unwrap_or(if is_ams_ht_id(self.ams_id) {
            AmsUnitModel::AmsHt
        } else {
            AmsUnitModel::Ams2Pro
        });
        let (min_temp, max_temp) = assumed_model.dry_temp_range().ok_or_else(|| {
            Error::ModelMismatch("attached AMS unit has no drying chamber".into())
        })?;
        if temp < min_temp || temp > max_temp {
            return Err(Error::InvalidArgument(
                format!(
                    "AMS dry temperature {temp}°C outside this unit's {min_temp}-{max_temp}°C range"
                )
                .into(),
            ));
        }
        if let Some(hdt) =
            DryingMaterial::from_filament_type(&filament).map(DryingMaterial::heat_distortion_temp)
            && temp > hdt
        {
            log::warn!(
                "AMS dry temperature {temp}°C exceeds {}'s heat-distortion temperature {hdt}°C — \
                 BambuStudio refuses this on a loaded tray; sending as requested",
                filament
            );
        }

        let params = DryingParams {
            filament,
            temp,
            duration_hours,
            humidity: self.humidity,
            rotate_tray: self.rotate_tray,
            cooling_temp,
            close_power_conflict: self.close_power_conflict,
        };
        let ams_id = wire_ams_id(self.ams_id);
        self.client
            .dispatch(|seq| AmsFilamentDryingRequest::start(ams_id, params.clone(), seq))
            .await
    }
}
