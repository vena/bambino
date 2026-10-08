//! Persistent printer settings: a gated setter and a cached getter for each.
//!
//! Covers the eight `print_option` settings plus the door-open check (`set_door_stat`), idle
//! heating protection (`set_against_continued_heating_mode`) and storing sent files
//! (`print_cache_set`), which share the same `print.cfg` read-back.
//!
//! Every `print_option` ack reports success, even on a model without the feature
//! [REF-MQTT-TELEMETRY], so the ack proves nothing. Each setter therefore refuses up front
//! when [`Capabilities`](super::Capabilities) says the setting is unsupported, and telemetry,
//! through the matching getter, is the only confirmation that a change took.
//!
//! **Settle window.** The `print_option` getters take an accepted reply's value at once (see
//! `ReplySettings`), but the printer can keep reporting the old value for about 3 s (one or two
//! status frames) after a change, and the next status frame replaces the reply's value.
//! BambuStudio and bambuddy both ignore telemetry for that long after sending. Wait that long
//! before trusting a getter to confirm a setter.

#[cfg(not(feature = "std"))]
use alloc::format;

use crate::error::Error;
use crate::io::{AsyncIo, RawStreamFactory, TimerProvider, TlsConnector};
use crate::mqtt::commands::hardware::PRINT_OP_AUTO_RECOVERY;
use crate::mqtt::commands::{
    AirPrintDetectRequest, AirPurificationRequest, AutoRecoveryRequest, FilamentBackupRequest,
    FilamentTangleDetectRequest, NozzleBlobDetectRequest, PromptSoundRequest,
    SmartNozzleBlobDetectRequest,
};
use crate::mqtt::commands::{
    DoorOpenCheckRequest, IdleHeatingProtectionRequest, StoreSentFilesRequest, XcamControlRequest,
};
use crate::quirks::Support;
use crate::types::control::{
    AirPurificationMode, DoorOpenCheck, IdleHeatingProtection, NozzleBlobDetectMode,
    XcamHaltSensitivity, XcamModule,
};
use crate::types::telemetry::bits::{self, SettingBits};
use crate::types::telemetry::{deserialize_permissive_opt_bool, deserialize_permissive_opt_int};
use serde::Deserialize;

use super::hardware::require;
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
    /// Turns prompt notification sounds on or off.
    ///
    /// Confirm with [`prompt_sound_enabled()`](Self::prompt_sound_enabled) after the settle
    /// window described in the module docs.
    ///
    /// # Errors
    ///
    /// [`Error::ModelMismatch`] when
    /// [`Capabilities::prompt_sound_support`](super::Capabilities::prompt_sound_support) is
    /// `false`.
    pub async fn set_prompt_sound(&mut self, enable: bool) -> Result<CommandHandle, Error> {
        refuse_unless(self.capabilities().prompt_sound_support(), "prompt sound")?;
        self.dispatch(|seq| PromptSoundRequest::new(enable, seq))
            .await
    }

    /// Turns step-loss auto-recovery on or off.
    ///
    /// Confirm with [`auto_recovery_enabled()`](Self::auto_recovery_enabled) after the settle
    /// window described in the module docs.
    ///
    /// # Errors
    ///
    /// [`Error::ModelMismatch`] when
    /// [`Capabilities::auto_recovery_support`](super::Capabilities::auto_recovery_support) is
    /// `false`.
    pub async fn set_auto_recovery(&mut self, enable: bool) -> Result<CommandHandle, Error> {
        refuse_unless(
            self.capabilities().auto_recovery_support(),
            "step-loss auto-recovery",
        )?;
        self.dispatch(|seq| AutoRecoveryRequest::new(enable, seq))
            .await
    }

    /// Turns AMS Filament Backup (auto-refill from a matching spool) on or off.
    ///
    /// Confirm with [`filament_backup_enabled()`](Self::filament_backup_enabled) after the
    /// settle window described in the module docs.
    ///
    /// # Errors
    ///
    /// [`Error::ModelMismatch`] when
    /// [`Capabilities::filament_backup_support`](super::Capabilities::filament_backup_support)
    /// is `false`.
    pub async fn set_filament_backup(&mut self, enable: bool) -> Result<CommandHandle, Error> {
        refuse_unless(
            self.capabilities().filament_backup_support(),
            "Filament Backup",
        )?;
        self.dispatch(|seq| FilamentBackupRequest::new(enable, seq))
            .await
    }

    /// Turns filament tangle detection on or off.
    ///
    /// Refused until the printer has reported support, so poll telemetry after connecting
    /// first. Confirm with
    /// [`filament_tangle_detect_enabled()`](Self::filament_tangle_detect_enabled) after the
    /// settle window described in the module docs.
    ///
    /// # Errors
    ///
    /// [`Error::ModelMismatch`] when
    /// [`Capabilities::filament_tangle_detect_support`](super::Capabilities::filament_tangle_detect_support)
    /// is `false`.
    pub async fn set_filament_tangle_detect(
        &mut self,
        enable: bool,
    ) -> Result<CommandHandle, Error> {
        refuse_unless(
            self.capabilities().filament_tangle_detect_support(),
            "filament tangle detection",
        )?;
        self.dispatch(|seq| FilamentTangleDetectRequest::new(enable, seq))
            .await
    }

    /// Turns nozzle blob detection (the original, on/off form) on or off.
    ///
    /// Refused until the printer has reported support, so poll telemetry after connecting
    /// first. Confirm with [`nozzle_blob_detect_enabled()`](Self::nozzle_blob_detect_enabled)
    /// after the settle window described in the module docs.
    ///
    /// # Errors
    ///
    /// [`Error::ModelMismatch`] when
    /// [`Capabilities::nozzle_blob_detect_support`](super::Capabilities::nozzle_blob_detect_support)
    /// is `false`.
    pub async fn set_nozzle_blob_detect(&mut self, enable: bool) -> Result<CommandHandle, Error> {
        refuse_unless(
            self.capabilities().nozzle_blob_detect_support(),
            "nozzle blob detection",
        )?;
        self.dispatch(|seq| NozzleBlobDetectRequest::new(enable, seq))
            .await
    }

    /// Sets the smart nozzle blob detection mode (off, on, or auto).
    ///
    /// Refused until the printer has reported support, so poll telemetry after connecting
    /// first. Confirm with
    /// [`smart_nozzle_blob_detect_mode()`](Self::smart_nozzle_blob_detect_mode) after the
    /// settle window described in the module docs.
    ///
    /// # Errors
    ///
    /// [`Error::ModelMismatch`] when
    /// [`Capabilities::smart_nozzle_blob_detect_support`](super::Capabilities::smart_nozzle_blob_detect_support)
    /// is `false`.
    pub async fn set_smart_nozzle_blob_detect(
        &mut self,
        mode: NozzleBlobDetectMode,
    ) -> Result<CommandHandle, Error> {
        refuse_unless(
            self.capabilities().smart_nozzle_blob_detect_support(),
            "smart nozzle blob detection",
        )?;
        self.dispatch(|seq| SmartNozzleBlobDetectRequest::new(mode, seq))
            .await
    }

    /// Turns non-visual air-printing detection on or off.
    ///
    /// The detector BambuStudio shows in AMS settings on A1/A1 Mini and in print options
    /// elsewhere; not the camera's AI air-printing detector. Refused until the printer has
    /// reported support, so poll telemetry after connecting first. Confirm with
    /// [`air_print_detect_enabled()`](Self::air_print_detect_enabled) after the settle window
    /// described in the module docs.
    ///
    /// # Errors
    ///
    /// [`Error::ModelMismatch`] when
    /// [`Capabilities::air_print_detect_support`](super::Capabilities::air_print_detect_support)
    /// is `false`.
    pub async fn set_air_print_detect(&mut self, enable: bool) -> Result<CommandHandle, Error> {
        refuse_unless(
            self.capabilities().air_print_detect_support(),
            "air-printing detection",
        )?;
        self.dispatch(|seq| AirPrintDetectRequest::new(enable, seq))
            .await
    }

    /// Sets where chamber air is purified at the end of every print.
    ///
    /// A persistent setting, unrelated to
    /// [`disable_air_purification()`](Self::disable_air_purification), which answers an error
    /// dialog by stopping purification once, now. Refused until the printer has reported
    /// support, so poll telemetry after connecting first. Confirm with
    /// [`air_purification_mode()`](Self::air_purification_mode) after the settle window
    /// described in the module docs.
    ///
    /// # Errors
    ///
    /// [`Error::ModelMismatch`] when
    /// [`Capabilities::air_purification_support`](super::Capabilities::air_purification_support)
    /// is `false`.
    pub async fn set_air_purification(
        &mut self,
        mode: AirPurificationMode,
    ) -> Result<CommandHandle, Error> {
        refuse_unless(
            self.capabilities().air_purification_support(),
            "end-of-print air purification",
        )?;
        self.dispatch(|seq| AirPurificationRequest::new(mode, seq))
            .await
    }

    /// Sets what the printer does when its door opens mid-print.
    ///
    /// Confirm with [`door_open_check()`](Self::door_open_check) after the settle window
    /// described in the module docs.
    ///
    /// # Errors
    ///
    /// [`Error::ModelMismatch`] when
    /// [`Capabilities::door_open_check_support`](super::Capabilities::door_open_check_support)
    /// is `false`.
    pub async fn set_door_open_check(
        &mut self,
        mode: DoorOpenCheck,
    ) -> Result<CommandHandle, Error> {
        refuse_unless(
            self.capabilities().door_open_check_support(),
            "door-open check",
        )?;
        self.dispatch(|seq| DoorOpenCheckRequest::new(mode, seq))
            .await
    }

    /// Turns idle heating protection on or off.
    ///
    /// Probably has no effect while [`idle_heating_protection()`](Self::idle_heating_protection)
    /// reads [`IdleHeatingProtection::Unavailable`]. Refused until the printer has reported
    /// support, so poll telemetry after connecting first.
    ///
    /// # Errors
    ///
    /// [`Error::ModelMismatch`] when
    /// [`Capabilities::idle_heating_protection_support`](super::Capabilities::idle_heating_protection_support)
    /// is `false`.
    pub async fn set_idle_heating_protection(
        &mut self,
        enable: bool,
    ) -> Result<CommandHandle, Error> {
        refuse_unless(
            self.capabilities().idle_heating_protection_support(),
            "idle heating protection",
        )?;
        self.dispatch(|seq| IdleHeatingProtectionRequest::new(enable, seq))
            .await
    }

    /// Sets whether files sent from Bambu Studio, Bambu Handy and MakerWorld are kept on external storage.
    ///
    /// Confirm with [`store_sent_files_enabled()`](Self::store_sent_files_enabled) after the
    /// settle window described in the module docs.
    ///
    /// # Errors
    ///
    /// [`Error::ModelMismatch`] when
    /// [`ModelQuirks::supports_store_sent_files`](crate::quirks::ModelQuirks::supports_store_sent_files)
    /// is `false`.
    pub async fn set_store_sent_files(&mut self, store: bool) -> Result<CommandHandle, Error> {
        require(self.quirks().supports_store_sent_files(), || {
            "storing sent files not available on this model".into()
        })?;
        self.dispatch(|seq| StoreSentFilesRequest::new(store, seq))
            .await
    }

    /// Turns one camera detector on or off, optionally setting how eagerly it halts the print.
    ///
    /// Read the result back through [`xcam()`](Self::xcam) (the detector accessors on
    /// [`XcamTelemetry`](crate::types::XcamTelemetry)) or, for first-layer inspection,
    /// [`first_layer_inspection_enabled()`](Self::first_layer_inspection_enabled), after the
    /// settle window described in the module docs. `xcam` arrives only in full status reports.
    /// bambuddy notes the firmware links spaghetti and pile-up sensitivity, so setting one may
    /// change both; that is unconfirmed.
    ///
    /// # Errors
    ///
    /// - [`Error::InvalidArgument`] for a `sensitivity` on a module that takes none
    ///   ([`XcamModule::takes_sensitivity`]).
    /// - [`Error::ModelMismatch`] when
    ///   [`Capabilities::xcam_module_support`](super::Capabilities::xcam_module_support) is
    ///   `false`.
    pub async fn set_xcam_detector(
        &mut self,
        module: XcamModule,
        enable: bool,
        sensitivity: Option<XcamHaltSensitivity>,
    ) -> Result<CommandHandle, Error> {
        if sensitivity.is_some() && !module.takes_sensitivity() {
            return Err(Error::InvalidArgument(
                format!("{module} takes no halt sensitivity").into(),
            ));
        }
        refuse_unless(
            self.capabilities().xcam_module_support(module),
            module.as_wire(),
        )?;
        self.dispatch(|seq| XcamControlRequest::new(module, enable, sensitivity, seq))
            .await
    }

    /// Whether prompt sounds are on, as last reported (`print.cfg` bit 22, else `home_flag` bit 17).
    ///
    /// `None` before any telemetry carrying the setting. See the module docs for the settle
    /// window after a change.
    #[must_use]
    pub fn prompt_sound_enabled(&self) -> Option<bool> {
        self.core
            .cache
            .reply_settings
            .prompt_sound
            .or_else(|| self.setting(bits::PROMPT_SOUND))
    }

    /// Whether step-loss auto-recovery is on, as last reported (`print.cfg` bit 16, else `home_flag` bit 4).
    #[must_use]
    pub fn auto_recovery_enabled(&self) -> Option<bool> {
        self.core
            .cache
            .reply_settings
            .auto_recovery
            .or_else(|| self.setting(bits::AUTO_RECOVERY))
    }

    /// Whether AMS Filament Backup is on, as last reported (`print.cfg` bit 18, else `home_flag` bit 10).
    #[must_use]
    pub fn filament_backup_enabled(&self) -> Option<bool> {
        self.core
            .cache
            .reply_settings
            .filament_backup
            .or_else(|| self.setting(bits::FILAMENT_BACKUP))
    }

    /// Whether filament tangle detection is on, as last reported (`print.cfg` bit 23, else `home_flag` bit 20).
    #[must_use]
    pub fn filament_tangle_detect_enabled(&self) -> Option<bool> {
        self.core
            .cache
            .reply_settings
            .filament_tangle_detect
            .or_else(|| self.setting(bits::FILAMENT_TANGLE_DETECT))
    }

    /// Whether on/off nozzle blob detection is on, as last reported (`print.cfg` bit 24, else `home_flag` bit 24).
    #[must_use]
    pub fn nozzle_blob_detect_enabled(&self) -> Option<bool> {
        self.core
            .cache
            .reply_settings
            .nozzle_blob_detect
            .or_else(|| self.setting(bits::NOZZLE_BLOB_DETECT))
    }

    /// Whether non-visual air-printing detection is on, as last reported (`home_flag` bit 28).
    #[must_use]
    pub fn air_print_detect_enabled(&self) -> Option<bool> {
        self.core
            .cache
            .reply_settings
            .air_print_detect
            .or_else(|| self.setting(bits::AIR_PRINT_DETECT))
    }

    /// The smart nozzle blob detection mode, as last reported (`print.cfg` bits 43-44).
    ///
    /// `None` before any `cfg`, always on P1 and A1 (which send none), and for the unassigned
    /// code `3`.
    #[must_use]
    pub fn smart_nozzle_blob_detect_mode(&self) -> Option<NozzleBlobDetectMode> {
        self.core
            .cache
            .reply_settings
            .smart_nozzle_blob_detect
            .or_else(|| {
                NozzleBlobDetectMode::from_code(self.cfg_field(bits::CFG_SMART_NOZZLE_BLOB_DETECT)?)
            })
    }

    /// The end-of-print air purification mode, as last reported (`print.cfg` bits 36-37).
    ///
    /// `None` before any `cfg`, always on P1 and A1 (which send none), and for the unassigned
    /// code `3`.
    #[must_use]
    pub fn air_purification_mode(&self) -> Option<AirPurificationMode> {
        self.core
            .cache
            .reply_settings
            .air_purification
            .or_else(|| AirPurificationMode::from_code(self.cfg_field(bits::CFG_AIR_PURIFICATION)?))
    }

    /// The door-open check mode, as last reported (`print.cfg` bits 20-21).
    ///
    /// `None` before any `cfg`, always on P1 and A1 (which send none), and for the unassigned
    /// code `3`.
    #[must_use]
    pub fn door_open_check(&self) -> Option<DoorOpenCheck> {
        DoorOpenCheck::from_code(self.cfg_field(bits::CFG_DOOR_OPEN_CHECK)?)
    }

    /// Idle heating protection, as last reported (`print.cfg` bits 32-33).
    ///
    /// `None` before any `cfg`, always on P1 and A1 (which send none), and for the unassigned
    /// code `3`.
    #[must_use]
    pub fn idle_heating_protection(&self) -> Option<IdleHeatingProtection> {
        IdleHeatingProtection::from_code(self.cfg_field(bits::CFG_IDLE_HEATING_PROTECTION)?)
    }

    /// Whether first-layer inspection is on, as last reported (`print.cfg` bit 12, else `xcam.first_layer_inspector`).
    #[must_use]
    pub fn first_layer_inspection_enabled(&self) -> Option<bool> {
        self.setting(bits::FIRST_LAYER_INSPECT)
            .or_else(|| self.core.cache.last_xcam.as_ref()?.first_layer_inspector)
    }

    /// Whether sent files are kept on external storage, as last reported (`print.cfg` bit 19).
    #[must_use]
    pub fn store_sent_files_enabled(&self) -> Option<bool> {
        self.setting(bits::STORE_SENT_FILES)
    }

    /// Caches the setting values an accepted `print_option` reply carries, for each supported setting.
    ///
    /// Called for replies to this client's commands and to other clients' alike. `auto_recovery`
    /// wins over `option` when both are present, as in BambuStudio's and OrcaSlicer's parsers.
    pub(super) fn apply_print_option_reply(&mut self, payload: &[u8]) {
        let Ok(PrintOptionReply { print: fields }) = serde_json::from_slice(payload) else {
            return;
        };
        let caps = self.capabilities();
        let auto_recovery = fields
            .auto_recovery
            .or_else(|| Some((fields.option? >> PRINT_OP_AUTO_RECOVERY) & 1 != 0));
        let reply = ReplySettings {
            prompt_sound: if_supported(caps.prompt_sound_support(), fields.sound_enable),
            auto_recovery: if_supported(caps.auto_recovery_support(), auto_recovery),
            filament_backup: if_supported(
                caps.filament_backup_support(),
                fields.auto_switch_filament,
            ),
            filament_tangle_detect: if_supported(
                caps.filament_tangle_detect_support(),
                fields.filament_tangle_detect,
            ),
            nozzle_blob_detect: if_supported(
                caps.nozzle_blob_detect_support(),
                fields.nozzle_blob_detect,
            ),
            smart_nozzle_blob_detect: if_supported(
                caps.smart_nozzle_blob_detect_support(),
                fields
                    .nozzle_blob_detect_v2
                    .and_then(NozzleBlobDetectMode::from_code),
            ),
            air_print_detect: if_supported(
                caps.air_print_detect_support(),
                fields.air_print_detect,
            ),
            air_purification: if_supported(
                caps.air_purification_support(),
                fields
                    .air_purification
                    .and_then(AirPurificationMode::from_code),
            ),
        };
        self.core.cache.reply_settings.merge(reply);
    }

    /// Reads one boolean setting from the cached `cfg`, else the `home_flag` of the last full status report.
    ///
    /// Heartbeat frames carry a partial `home_flag`, so only a full report's counts. It is read
    /// from any connection, not just the current one: a setting persists across a reconnect, and
    /// the connect-time pushall refreshes it.
    fn setting(&self, setting: SettingBits) -> Option<bool> {
        setting.read(
            self.core.cache.last_cfg.as_deref(),
            self.core.cache.last_full_home_flag,
        )
    }

    /// Reads a two-bit mode field from the cached `cfg`.
    fn cfg_field(&self, low: u32) -> Option<u32> {
        bits::hex_field(self.core.cache.last_cfg.as_deref()?, low, 2)
    }
}

/// `print_option` setting values carried by an accepted command reply.
///
/// The printer's reply echoes the setting field it was sent (a P1S `ack-probe` capture shows
/// `sound_enable` coming back; BambuStudio and OrcaSlicer read `option`/`auto_recovery` the same
/// way), and arrives well before a status frame reflects the change. Each value here is held
/// until the next status frame that carries settings (`print.cfg` or a full report's
/// `home_flag`) clears them all. That frame may still carry the old value for about 3 s, so a
/// getter can flip back briefly; BambuStudio's 3 s hold is deliberately not reproduced.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ReplySettings {
    prompt_sound: Option<bool>,
    auto_recovery: Option<bool>,
    filament_backup: Option<bool>,
    filament_tangle_detect: Option<bool>,
    nozzle_blob_detect: Option<bool>,
    smart_nozzle_blob_detect: Option<NozzleBlobDetectMode>,
    air_print_detect: Option<bool>,
    air_purification: Option<AirPurificationMode>,
}

impl ReplySettings {
    /// Overwrites every setting `newer` carries, keeping the rest.
    fn merge(&mut self, newer: Self) {
        self.prompt_sound = newer.prompt_sound.or(self.prompt_sound);
        self.auto_recovery = newer.auto_recovery.or(self.auto_recovery);
        self.filament_backup = newer.filament_backup.or(self.filament_backup);
        self.filament_tangle_detect = newer.filament_tangle_detect.or(self.filament_tangle_detect);
        self.nozzle_blob_detect = newer.nozzle_blob_detect.or(self.nozzle_blob_detect);
        self.smart_nozzle_blob_detect = newer
            .smart_nozzle_blob_detect
            .or(self.smart_nozzle_blob_detect);
        self.air_print_detect = newer.air_print_detect.or(self.air_print_detect);
        self.air_purification = newer.air_purification.or(self.air_purification);
    }
}

/// The setting fields a `print_option` reply can carry, each read permissively.
#[derive(Debug, Default, Deserialize)]
struct PrintOptionReplyFields {
    #[serde(default, deserialize_with = "deserialize_permissive_opt_bool")]
    sound_enable: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_permissive_opt_bool")]
    auto_recovery: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_permissive_opt_int")]
    option: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_permissive_opt_bool")]
    auto_switch_filament: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_permissive_opt_bool")]
    filament_tangle_detect: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_permissive_opt_bool")]
    nozzle_blob_detect: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_permissive_opt_int")]
    nozzle_blob_detect_v2: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_permissive_opt_bool")]
    air_print_detect: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_permissive_opt_int")]
    air_purification: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct PrintOptionReply {
    print: PrintOptionReplyFields,
}

/// `value` if `support` allows the setting, else `None`: a reply acks even a setting the printer ignores.
fn if_supported<T>(support: Support, value: Option<T>) -> Option<T> {
    value.filter(|_| support.is_supported())
}

/// Fails with [`Error::ModelMismatch`] naming `setting` unless `support` allows it.
fn refuse_unless(support: Support, setting: &str) -> Result<(), Error> {
    require(support.is_supported(), || {
        format!("{setting} not available on this printer ({support:?})")
    })
}
