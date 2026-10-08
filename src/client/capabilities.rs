//! # Client-Scoped Capabilities
//!
//! [`Capabilities`] answers "can this printer do X" with the client's own cached telemetry
//! already supplied.
//!
//! A [`ModelQuirks`] method that depends on what the machine
//! reported takes a [`QuirkContext`]. That is what keeps a single answer to each question — a
//! caller cannot get a stale reading by forgetting to compose the report, because the context is
//! required. The cost is that every such call needs a context built and threaded in, and the
//! obvious call (`client.quirks().foo(..)`) makes the caller assemble it.
//!
//! This view removes that cost: it holds the model's quirks alongside a context built from the
//! client's cache, so `client.capabilities().supports_ams_remote_drying()` takes no arguments
//! and cannot be given the wrong ones.
//!
//! **Only context-taking quirks are forwarded here.** Everything a model answers on its own —
//! build volume, fan layout, camera protocol — stays on
//! [`PrinterClient::quirks()`](crate::client::PrinterClient::quirks), where no telemetry could change
//! the answer and a bare `&'static ModelQuirks` is the honest shape.
//!
//! The context is a snapshot taken when the view is created. It reflects what the client had
//! cached at that moment, so a `Capabilities` held across a
//! [`poll_telemetry()`](crate::client::PrinterClient::poll_telemetry) goes stale — build a fresh one per
//! question rather than storing it.

use crate::quirks::{ModelQuirks, QuirkContext, Support};
use crate::types::control::XcamModule;

/// Capability answers for one printer, with its cached telemetry already supplied.
///
/// Created by [`PrinterClient::capabilities()`](crate::client::PrinterClient::capabilities). See the
/// [module docs](self) for what is and isn't forwarded here.
#[derive(Clone, Copy)]
pub struct Capabilities<'a> {
    quirks: &'static ModelQuirks,
    context: QuirkContext<'a>,
}

impl<'a> Capabilities<'a> {
    /// Builds a view over a model's quirks and a context.
    pub(crate) fn new(quirks: &'static ModelQuirks, context: QuirkContext<'a>) -> Self {
        Self { quirks, context }
    }

    /// The context these answers are resolved against.
    ///
    /// Useful for asking the same question of a different model, or for seeing which inputs were
    /// actually available — an answer resolved with `firmware: None` rests on a model rule
    /// rather than on anything the printer said.
    #[must_use]
    pub fn context(&self) -> &QuirkContext<'a> {
        &self.context
    }

    /// The underlying model quirks, for the capabilities that take no context.
    #[must_use]
    pub fn quirks(&self) -> &'static ModelQuirks {
        self.quirks
    }

    /// Whether this printer honors `ams_filament_drying` sent over MQTT.
    ///
    /// Resolves the printer's reported `fun2` bit 5 against the model's own rules — never
    /// supported on A1/A1 Mini, P1P/P1S and X1/X1C, firmware-gated on H2D/H2D Pro/H2S/H2C/P2S/X2D,
    /// always on A2L, assumed allowed elsewhere. See
    /// [`ModelQuirks::ams_remote_drying_support`]
    /// for the sourcing.
    ///
    /// **Gate UI on this rather than on a model check.** It is the same value
    /// [`DryingCycle::send`](crate::client::DryingCycle::send) tests, so a control offered on the
    /// strength of it will not then be refused.
    ///
    /// On a firmware-gated model an unread version does **not** deny the capability — it falls
    /// back to the model's answer, and only a version actually read and found older refuses.
    /// [`connect_mqtt()`](crate::client::PrinterClient::connect_mqtt) and
    /// [`connect_all()`](crate::client::PrinterClient::connect_all) fetch the version for you, so a
    /// normally-connected client has it; a caller relying on lazy connection gets the
    /// model-rule answer instead.
    #[must_use]
    pub fn supports_ams_remote_drying(&self) -> bool {
        self.quirks
            .ams_remote_drying_support(&self.context)
            .is_supported()
    }

    /// Remote-drying support with its provenance attached.
    ///
    /// The same answer as [`supports_ams_remote_drying`](Self::supports_ams_remote_drying), plus
    /// whether it came from the printer ([`Support::Reported`]), from its firmware version or a
    /// model rule ([`Support::Inferred`]), or is the default because nothing was known yet
    /// ([`Support::Assumed`]). Use it to tell "this printer can't" from "ask again once
    /// connected".
    #[must_use]
    pub fn ams_remote_drying_support(&self) -> Support {
        self.quirks.ams_remote_drying_support(&self.context)
    }

    /// Whether an AMS drying cycle can run while a print is in progress.
    ///
    /// Strictly narrower than [`supports_ams_remote_drying`](Self::supports_ams_remote_drying),
    /// and defaults to `false` when the firmware version is unknown — except on X2D and A2L,
    /// whose earliest firmware already has the feature, so they report `true` before
    /// `get_version()` completes. See
    /// [`ModelQuirks::ams_drying_while_printing_support`] for the sourcing.
    #[must_use]
    pub fn supports_ams_drying_while_printing(&self) -> bool {
        self.quirks
            .ams_drying_while_printing_support(&self.context)
            .is_supported()
    }

    /// Drying-while-printing support with its provenance attached.
    ///
    /// The same answer as
    /// [`supports_ams_drying_while_printing`](Self::supports_ams_drying_while_printing).
    #[must_use]
    pub fn ams_drying_while_printing_support(&self) -> Support {
        self.quirks.ams_drying_while_printing_support(&self.context)
    }

    /// Prompt sound support — see [`ModelQuirks::prompt_sound_support`].
    #[must_use]
    pub fn prompt_sound_support(&self) -> Support {
        self.quirks.prompt_sound_support(&self.context)
    }

    /// Step-loss auto-recovery support — see [`ModelQuirks::auto_recovery_support`].
    #[must_use]
    pub fn auto_recovery_support(&self) -> Support {
        self.quirks.auto_recovery_support(&self.context)
    }

    /// AMS Filament Backup support — see [`ModelQuirks::filament_backup_support`].
    #[must_use]
    pub fn filament_backup_support(&self) -> Support {
        self.quirks.filament_backup_support(&self.context)
    }

    /// Filament tangle detection support — see [`ModelQuirks::filament_tangle_detect_support`].
    #[must_use]
    pub fn filament_tangle_detect_support(&self) -> Support {
        self.quirks.filament_tangle_detect_support(&self.context)
    }

    /// On/off nozzle blob detection support — see [`ModelQuirks::nozzle_blob_detect_support`].
    #[must_use]
    pub fn nozzle_blob_detect_support(&self) -> Support {
        self.quirks.nozzle_blob_detect_support(&self.context)
    }

    /// Smart nozzle blob detection support — see [`ModelQuirks::smart_nozzle_blob_detect_support`].
    #[must_use]
    pub fn smart_nozzle_blob_detect_support(&self) -> Support {
        self.quirks.smart_nozzle_blob_detect_support(&self.context)
    }

    /// Non-visual air-printing detection support — see [`ModelQuirks::air_print_detect_support`].
    #[must_use]
    pub fn air_print_detect_support(&self) -> Support {
        self.quirks.air_print_detect_support(&self.context)
    }

    /// End-of-print air purification support — see [`ModelQuirks::air_purification_support`].
    #[must_use]
    pub fn air_purification_support(&self) -> Support {
        self.quirks.air_purification_support(&self.context)
    }

    /// Door-open check support — see [`ModelQuirks::door_open_check_support`].
    #[must_use]
    pub fn door_open_check_support(&self) -> Support {
        self.quirks.door_open_check_support(&self.context)
    }

    /// Idle heating protection support — see [`ModelQuirks::idle_heating_protection_support`].
    #[must_use]
    pub fn idle_heating_protection_support(&self) -> Support {
        self.quirks.idle_heating_protection_support(&self.context)
    }

    /// Camera detector support — see [`ModelQuirks::xcam_module_support`].
    #[must_use]
    pub fn xcam_module_support(&self, module: XcamModule) -> Support {
        self.quirks.xcam_module_support(module, &self.context)
    }
}

impl core::fmt::Debug for Capabilities<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // `ModelQuirks` is not Debug, and the useful content is the resolved answers plus
        // the inputs they came from.
        f.debug_struct("Capabilities")
            .field("context", &self.context)
            .field(
                "ams_remote_drying_support",
                &self.ams_remote_drying_support(),
            )
            .field(
                "ams_drying_while_printing_support",
                &self.ams_drying_while_printing_support(),
            )
            .finish()
    }
}
