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
//! the answer and a bare `&'static dyn ModelQuirks` is the honest shape.
//!
//! The context is a snapshot taken when the view is created. It reflects what the client had
//! cached at that moment, so a `Capabilities` held across a
//! [`poll_telemetry()`](crate::client::PrinterClient::poll_telemetry) goes stale — build a fresh one per
//! question rather than storing it.

use crate::quirks::{ModelQuirks, QuirkContext};

/// Capability answers for one printer, with its cached telemetry already supplied.
///
/// Created by [`PrinterClient::capabilities()`](crate::client::PrinterClient::capabilities). See the
/// [module docs](self) for what is and isn't forwarded here.
#[derive(Clone, Copy)]
pub struct Capabilities<'a> {
    quirks: &'static dyn ModelQuirks,
    context: QuirkContext<'a>,
}

impl<'a> Capabilities<'a> {
    /// Builds a view over a model's quirks and a context.
    pub(crate) fn new(quirks: &'static dyn ModelQuirks, context: QuirkContext<'a>) -> Self {
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
    pub fn quirks(&self) -> &'static dyn ModelQuirks {
        self.quirks
    }

    /// Whether this printer honors `ams_filament_drying` sent over MQTT.
    ///
    /// Resolves the printer's reported `fun2` bit 5 against the model's own rules — never
    /// supported on A1/A1 Mini and P1P/P1S, firmware-gated on X1C/P2S/H2D/H2S/H2C, allowed
    /// elsewhere. See
    /// [`ModelQuirks::supports_ams_remote_drying`]
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
        self.quirks.supports_ams_remote_drying(&self.context)
    }
}

impl core::fmt::Debug for Capabilities<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // `dyn ModelQuirks` is not Debug, and the useful content is the resolved answers plus
        // the inputs they came from.
        f.debug_struct("Capabilities")
            .field("context", &self.context)
            .field(
                "supports_ams_remote_drying",
                &self.supports_ams_remote_drying(),
            )
            .finish()
    }
}
