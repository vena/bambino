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

use crate::error::Error;
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};
use crate::types::DryingMaterial;
use crate::types::drying::DEFAULT_COMMAND_COOLING_TEMP;
use crate::types::telemetry::AmsUnitModel;
use crate::types::telemetry::ams::{
    AMS_DRY_TEMP_MIN, AMS_HT_DRY_TEMP_MAX, AMS_STANDARD_DRY_TEMP_MAX,
};

use super::{CommandHandle, PrinterClient};

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
    temp: u32,
    duration_hours: u32,
    humidity: u32,
    rotate_tray: bool,
    cooling_temp: i32,
    close_power_conflict: bool,
    filament: String,
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
    /// `temp` and `duration_hours` default to `0`, which
    /// [`send()`](Self::send) rejects — a cycle needs a real temperature, and silently picking
    /// one would start a heating cycle nobody asked for. Set them with
    /// [`material()`](Self::material) or explicitly.
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
            temp: 0,
            duration_hours: 0,
            humidity: 0,
            rotate_tray: false,
            // BambuStudio's own fallback when a tray's filament resolves to no preset
            // (`AMSDryControl.cpp:813`), not a zero.
            cooling_temp: DEFAULT_COMMAND_COOLING_TEMP,
            close_power_conflict: false,
            filament: String::new(),
        }
    }

    /// Fills temperature, duration, cooling temperature and the filament name from the vendor's
    /// published parameters for `material` on `unit`.
    ///
    /// Sets four fields at once, which is the whole reason this builder exists — the same choice
    /// on a positional call means threading three numbers and a string into four of nine slots.
    ///
    /// Assumes an idle printer. For a cycle that runs alongside a print, follow with
    /// [`printing()`](Self::printing), which re-reads the lower while-printing column.
    ///
    /// A material with no published parameters for this unit (any unit without a drying chamber)
    /// leaves the values untouched, so [`send()`](Self::send) still rejects rather than
    /// publishing a guess.
    pub fn material(mut self, material: DryingMaterial, unit: AmsUnitModel) -> Self {
        if let (Some(temp), Some(hours)) = (
            material.default_temp(unit, false),
            material.default_duration_hours(unit, false),
        ) {
            self.temp = temp;
            self.duration_hours = hours;
        }
        self.cooling_temp = material.command_cooling_temp();
        self.filament = material.wire_name().to_string();
        self
    }

    /// Re-reads the material's parameters from the while-printing column.
    ///
    /// Only meaningful after [`material()`](Self::material); on its own it does nothing, since
    /// there is no material to re-read. The printing column is lower because the AMS sits in the
    /// print's thermal envelope.
    pub fn printing(mut self, material: DryingMaterial, unit: AmsUnitModel) -> Self {
        if let (Some(temp), Some(hours)) = (
            material.default_temp(unit, true),
            material.default_duration_hours(unit, true),
        ) {
            self.temp = temp;
            self.duration_hours = hours;
        }
        self
    }

    /// Sets the drying temperature in °C, overriding any material default.
    pub fn temp(mut self, temp: u32) -> Self {
        self.temp = temp;
        self
    }

    /// Sets the cycle duration in whole hours, overriding any material default.
    pub fn duration_hours(mut self, hours: u32) -> Self {
        self.duration_hours = hours;
        self
    }

    /// Sets the filament type string sent as the wire `dry_filament` field.
    ///
    /// Free-form by design — the wire field is arbitrary text and BambuStudio sends the tray's
    /// own `filament_type`. Use this for a material [`DryingMaterial`] does not name.
    pub fn filament(mut self, filament: &str) -> Self {
        self.filament = filament.to_string();
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
    /// Defaults to [`DEFAULT_COMMAND_COOLING_TEMP`], and [`material()`](Self::material) sets it
    /// to that material's *softening* temperature — which is what the wire field actually
    /// carries, despite the profiles also having a similarly-named
    /// `filament_dev_drying_cooling_temperature` that BambuStudio never sends.
    pub fn cooling_temp(mut self, cooling_temp: i32) -> Self {
        self.cooling_temp = cooling_temp;
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

    /// Validates and publishes the cycle, returning the command's sequence ID [REF-AMS-DRYER].
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
    /// A1/A1 Mini or P1P/P1S, and below the minimum firmware on X1C/P2S/H2D/H2S/H2C. Such
    /// firmware acks this command `result: success` and silently discards it rather than driving
    /// the AMS heater.
    ///
    /// [`Error::ModelMismatch`] also when the addressed unit has no drying chamber — an
    /// external-spool sentinel (`254`/`255`), or a cached
    /// [`AmsUnitModel`] whose [`supports_drying`](AmsUnitModel::supports_drying) is `false`.
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
    pub async fn send(self) -> Result<CommandHandle, Error> {
        if self.temp == 0 {
            return Err(Error::InvalidArgument(
                "drying temperature not set — call .material(..) or .temp(..)".into(),
            ));
        }
        if self.duration_hours == 0 {
            return Err(Error::InvalidArgument(
                "drying duration not set — call .material(..) or .duration_hours(..)".into(),
            ));
        }
        if !self.client.supports_ams_remote_drying() {
            return Err(Error::ModelMismatch(
                "AMS drying is screen-only on this host printer — firmware acks this command but does not act on it".into(),
            ));
        }
        if !super::ams::is_valid_ams_id(self.ams_id) {
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

        let unit_model = self.client.cached_ams_unit_model(self.ams_id);
        if let Some(model) = unit_model
            && !model.supports_drying()
        {
            return Err(Error::ModelMismatch(
                "attached AMS unit has no drying chamber — only the AMS 2 Pro and AMS-HT can dry"
                    .into(),
            ));
        }

        // `dry_temp_range()` is the authority when the unit is known. Unobserved, fall back to
        // the address-derived ceiling — wrong for an original AMS or an AMS Lite at `0..=3`, but
        // that is exactly the case the gate above cannot rule on either.
        let (min_temp, max_temp) = unit_model.and_then(AmsUnitModel::dry_temp_range).unwrap_or(
            if (128..=135).contains(&self.ams_id) {
                (AMS_DRY_TEMP_MIN, AMS_HT_DRY_TEMP_MAX)
            } else {
                (AMS_DRY_TEMP_MIN, AMS_STANDARD_DRY_TEMP_MAX)
            },
        );
        let temp = self.temp;
        if temp < min_temp || temp > max_temp {
            return Err(Error::InvalidArgument(
                format!(
                    "AMS dry temperature {temp}°C outside this unit's {min_temp}-{max_temp}°C range"
                )
                .into(),
            ));
        }

        let Self {
            client,
            ams_id,
            duration_hours,
            humidity,
            rotate_tray,
            cooling_temp,
            close_power_conflict,
            filament,
            ..
        } = self;
        let ams_id = super::ams::wire_ams_id(ams_id);
        client
            .dispatch(|seq| {
                crate::mqtt::AmsFilamentDryingRequest::new(
                    ams_id,
                    1,
                    &filament,
                    temp,
                    duration_hours,
                    humidity,
                    rotate_tray,
                    cooling_temp,
                    close_power_conflict,
                    seq,
                )
            })
            .await
    }
}
