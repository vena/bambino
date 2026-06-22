//! # Physical Printer Quirks & Polymorphic Model Behaviors
//!
//! Defines the core `ModelQuirks` trait to isolate model-specific network
//! configurations, safety interlocks, and state-evaluation rules.
//!
//! Rather than polluting primary command execution paths with conditional checks,
//! behaviors are isolated polymorphically. The `ModelQuirks` trait is implemented
//! directly on the `BambuModel` discovery enumeration to permit efficient static
//! dispatch without requiring dynamic heap allocations (vtable pointers) [PLAN.md].

pub mod models;

use crate::discovery::BambuModel;
use crate::types::PrintTelemetry;

/// Polymorphic traits tracking model-specific hardware variations and transport exceptions.
pub trait ModelQuirks {
    /// Returns true if this model series requires plaintext transmissions on the
    /// FTPS passive data channel (PROT C) due to board limitations [REF-FTPS-CONN].
    fn uses_plaintext_ftps_data_channel(&self) -> bool;

    /// Returns true if this model series must restrict its TLS version strictly
    /// to TLS 1.2 to prevent session resumption failure [REF-FTPS-CONN].
    fn enforce_ftps_tls_1_2(&self) -> bool;

    /// Evaluates whether the physical front enclosure door is open based on
    /// model-specific sensor routing [REF-NET-DOOR].
    ///
    /// If the target model lacks an electronic door sensor switch, returns `false`.
    fn is_door_open(&self, telemetry: &PrintTelemetry) -> bool;

    /// Returns true if the physical machine chassis is equipped with an electronic
    /// front enclosure door open sensor switch.
    fn has_door_sensor(&self) -> bool;

    /// Returns the physical local TCP port used by the model's camera interface [REF-NET-PORTS].
    ///
    /// * Port `322`: High-capability RTSPS stream.
    /// * Port `6000`: Binary JPEG frame-buffer socket.
    fn camera_stream_port(&self) -> u16;

    /// Returns true if the model is an open-frame or entry-level machine lacking
    /// a physical chamber temperature sensor [REF-THER-DECODE].
    fn ignores_chamber_temperature(&self) -> bool;

    /// Returns true if the model series exhibits the idle state-machine bug where
    /// `stg_cur = 0` (Printing) is reported in idle phases [REF-MQTT-IDLEBUG].
    fn has_stg_cur_idle_bug(&self) -> bool;
}

impl ModelQuirks for BambuModel {
    fn uses_plaintext_ftps_data_channel(&self) -> bool {
        match self {
            BambuModel::A1 | BambuModel::A1Mini | BambuModel::A2L => true,
            _ => false,
        }
    }

    fn enforce_ftps_tls_1_2(&self) -> bool {
        match self {
            BambuModel::P2S | BambuModel::X2D => true,
            _ => false,
        }
    }

    fn is_door_open(&self, telemetry: &PrintTelemetry) -> bool {
        if !self.has_door_sensor() {
            return false;
        }
        match self {
            BambuModel::X1C | BambuModel::X1E => telemetry.is_door_open(true),
            BambuModel::X2D
            | BambuModel::P2S
            | BambuModel::H2D
            | BambuModel::H2DPro
            | BambuModel::H2C
            | BambuModel::H2S => telemetry.is_door_open(false),
            _ => false,
        }
    }

    fn has_door_sensor(&self) -> bool {
        match self {
            BambuModel::X1C
            | BambuModel::X1E
            | BambuModel::X2D
            | BambuModel::P2S
            | BambuModel::H2D
            | BambuModel::H2DPro
            | BambuModel::H2C
            | BambuModel::H2S => true,
            _ => false,
        }
    }

    fn camera_stream_port(&self) -> u16 {
        match self {
            BambuModel::A1
            | BambuModel::A1Mini
            | BambuModel::A2L
            | BambuModel::P1P
            | BambuModel::P1S => 6000,
            _ => 322,
        }
    }

    fn ignores_chamber_temperature(&self) -> bool {
        match self {
            BambuModel::P1P
            | BambuModel::P1S
            | BambuModel::A1
            | BambuModel::A1Mini
            | BambuModel::A2L => true,
            _ => false,
        }
    }

    fn has_stg_cur_idle_bug(&self) -> bool {
        match self {
            BambuModel::A1
            | BambuModel::A1Mini
            | BambuModel::A2L
            | BambuModel::P1P
            | BambuModel::P1S => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_dispatch_quirks() {
        let a1 = BambuModel::A1;
        assert!(a1.uses_plaintext_ftps_data_channel());
        assert!(!a1.enforce_ftps_tls_1_2());
        assert_eq!(a1.camera_stream_port(), 6000);
        assert!(a1.ignores_chamber_temperature());
        assert!(a1.has_stg_cur_idle_bug());

        let p2s = BambuModel::P2S;
        assert!(!p2s.uses_plaintext_ftps_data_channel());
        assert!(p2s.enforce_ftps_tls_1_2());
        assert_eq!(p2s.camera_stream_port(), 322);
        assert!(!p2s.ignores_chamber_temperature());
        assert!(!p2s.has_stg_cur_idle_bug());
    }
}
