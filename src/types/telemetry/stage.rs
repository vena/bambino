//! Typed decoding for the `stg_cur` / `stg` stage-ID space.

/// Generates [`PrintStage`] and its wire, label and pause tables from one row per stage, so the
/// four can't drift: `Variant = wire id, "BambuStudio label", paused: bool;`.
macro_rules! print_stages {
    (
        $(#[$enum_attr:meta])*
        pub enum PrintStage {
            $(
                $(#[$attr:meta])*
                $variant:ident = $id:literal, $label:literal, paused: $paused:literal;
            )*
        }
    ) => {
        $(#[$enum_attr])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum PrintStage {
            /// Idle. Wire sends `-1` on X1 and `255` on P1; both map here.
            Idle,
            $(
                $(#[$attr])*
                $variant,
            )*
            /// A stage id no upstream table covers. Carries the raw wire value.
            Unknown(i32),
        }

        impl PrintStage {
            /// Decodes a raw `stg_cur` / `stg` wire value.
            pub fn from_wire(value: i32) -> Self {
                match value {
                    -1 | 255 => Self::Idle,
                    $($id => Self::$variant,)*
                    other => Self::Unknown(other),
                }
            }

            /// Returns true if this stage is one of the paused states.
            pub fn is_paused(self) -> bool {
                match self {
                    $(Self::$variant => $paused,)*
                    Self::Idle | Self::Unknown(_) => false,
                }
            }

            /// Human-readable label, matching BambuStudio's own wording.
            ///
            /// Returns `None` for [`PrintStage::Unknown`] — the caller decides how to render an id
            /// no upstream table covers, rather than getting a fabricated label.
            pub fn label(self) -> Option<&'static str> {
                match self {
                    Self::Idle => Some("Idle"),
                    $(Self::$variant => Some($label),)*
                    Self::Unknown(_) => None,
                }
            }
        }
    };
}

print_stages! {
    /// A stage the printer reports in `stg_cur` (currently executing) or `stg` (queued).
    ///
    /// The wire carries bare integers. [`PrintStage::from_wire`] maps them to variants and leaves
    /// anything unrecognized in [`PrintStage::Unknown`] rather than failing, so a firmware that adds
    /// a stage id does not break decoding.
    ///
    /// # Read this before trusting a decoded value
    ///
    /// Decoding does not make `stg_cur` trustworthy on its own. A1 and P1 firmware reports
    /// `stg_cur = 0` ([`PrintStage::Printing`]) while genuinely idle [REF-MQTT-IDLEBUG], so the value
    /// means nothing unless `gcode_state` is `RUNNING` or `PAUSE`. Gate on that before displaying a
    /// stage, or use a helper that does it for you — a typed [`PrintStage::Printing`] handed to a UI
    /// looks authoritative in a way the raw `0` did not, which makes ignoring the gate *more*
    /// dangerous here, not less.
    ///
    /// [`PrintStage::Idle`] returning from a run in progress is also normal: once the last queued
    /// stage finishes, `stg_cur` reads idle for the remainder of the run while `mc_percent` keeps
    /// climbing. Completion is `gcode_state`/`mc_percent`, never this.
    ///
    /// # Verification
    ///
    /// Ids `0`–`77` come from BambuStudio's own `get_stage_string` (`DeviceManager.cpp`), the vendor
    /// client's table. bambuddy's independent `STAGE_NAMES` corroborates 69 of the 78 and contradicts
    /// none of them except id `74`, where bambuddy carries a self-described guess ("Preparing", noted
    /// upstream as seen on H2D) and BambuStudio is taken as authoritative. Ids `67`–`73`, `75` and
    /// `76` appear in BambuStudio alone.
    ///
    /// Directly observed on hardware here (P1S, firmware `01.10.00.00`): `0`, `1`, `3`, `14`, `25`
    /// and the `255` idle encoding. Everything else is upstream-sourced, not locally captured.
    ///
    /// Idle is reported as `-1` on X1 and `255` on P1; both normalize to [`PrintStage::Idle`], which
    /// is the split a raw `i32` would otherwise leak to every consumer.
    pub enum PrintStage {
        /// Printing.
        Printing = 0, "Printing", paused: false;
        /// Auto bed leveling.
        AutoBedLeveling = 1, "Auto bed leveling", paused: false;
        /// Heatbed preheating.
        HeatbedPreheating = 2, "Heatbed preheating", paused: false;
        /// Resonance sweep. Upstream's machine name for this is `sweeping_xy_mech_mode`.
        VibrationCompensation = 3, "Vibration compensation", paused: false;
        /// Changing filament.
        ChangingFilament = 4, "Changing filament", paused: false;
        /// M400 pause.
        M400Pause = 5, "M400 pause", paused: true;
        /// Paused (filament ran out).
        PausedFilamentRunout = 6, "Paused (filament ran out)", paused: true;
        /// Heating nozzle.
        HeatingNozzle = 7, "Heating nozzle", paused: false;
        /// Calibrating dynamic flow.
        CalibratingDynamicFlow = 8, "Calibrating dynamic flow", paused: false;
        /// Scanning bed surface.
        ScanningBedSurface = 9, "Scanning bed surface", paused: false;
        /// Inspecting first layer.
        InspectingFirstLayer = 10, "Inspecting first layer", paused: false;
        /// Identifying build plate type.
        IdentifyingBuildPlateType = 11, "Identifying build plate type", paused: false;
        /// Calibrating Micro Lidar.
        CalibratingMicroLidar = 12, "Calibrating Micro Lidar", paused: false;
        /// Homing toolhead.
        HomingToolhead = 13, "Homing toolhead", paused: false;
        /// Cleaning nozzle tip.
        CleaningNozzleTip = 14, "Cleaning nozzle tip", paused: false;
        /// Checking extruder temperature.
        CheckingExtruderTemperature = 15, "Checking extruder temperature", paused: false;
        /// Paused by the user.
        PausedByUser = 16, "Paused by the user", paused: true;
        /// Pause (front cover fall off).
        PausedFrontCoverFallOff = 17, "Pause (front cover fall off)", paused: true;
        /// Second micro-lidar calibration id. Upstream marks `12` and `18` as duplicated.
        CalibratingMicroLidarAlt = 18, "Calibrating Micro Lidar", paused: false;
        /// Calibrating flow ratio.
        CalibratingFlowRatio = 19, "Calibrating flow ratio", paused: false;
        /// Pause (nozzle temperature malfunction).
        PausedNozzleTemperatureMalfunction = 20, "Pause (nozzle temperature malfunction)", paused: true;
        /// Pause (heatbed temperature malfunction).
        PausedHeatbedTemperatureMalfunction = 21, "Pause (heatbed temperature malfunction)", paused: true;
        /// Filament unloading.
        FilamentUnloading = 22, "Filament unloading", paused: false;
        /// Pause (step loss).
        PausedStepLoss = 23, "Pause (step loss)", paused: true;
        /// Filament loading.
        FilamentLoading = 24, "Filament loading", paused: false;
        /// Motor noise cancellation.
        MotorNoiseCancellation = 25, "Motor noise cancellation", paused: false;
        /// Pause (AMS offline).
        PausedAmsOffline = 26, "Pause (AMS offline)", paused: true;
        /// Pause (low speed of the heatbreak fan).
        PausedLowHeatbreakFanSpeed = 27, "Pause (low speed of the heatbreak fan)", paused: true;
        /// Pause (chamber temperature control problem).
        PausedChamberTemperatureControlProblem = 28, "Pause (chamber temperature control problem)", paused: true;
        /// Cooling chamber.
        CoolingChamber = 29, "Cooling chamber", paused: false;
        /// Pause (Gcode inserted by user).
        PausedUserGcode = 30, "Pause (Gcode inserted by user)", paused: true;
        /// Motor noise showoff.
        MotorNoiseShowoff = 31, "Motor noise showoff", paused: false;
        /// Pause (nozzle clumping).
        PausedNozzleClumping = 32, "Pause (nozzle clumping)", paused: true;
        /// Pause (cutter error).
        PausedCutterError = 33, "Pause (cutter error)", paused: true;
        /// Pause (first layer error).
        PausedFirstLayerError = 34, "Pause (first layer error)", paused: true;
        /// Pause (nozzle clog).
        PausedNozzleClog = 35, "Pause (nozzle clog)", paused: true;
        /// Measuring motion precision.
        MeasuringMotionPrecision = 36, "Measuring motion precision", paused: false;
        /// Enhancing motion precision.
        EnhancingMotionPrecision = 37, "Enhancing motion precision", paused: false;
        /// Measure motion accuracy.
        MeasureMotionAccuracy = 38, "Measure motion accuracy", paused: false;
        /// Nozzle offset calibration.
        NozzleOffsetCalibration = 39, "Nozzle offset calibration", paused: false;
        /// High temperature auto bed leveling.
        HighTemperatureAutoBedLeveling = 40, "High temperature auto bed leveling", paused: false;
        /// Auto Check: Quick Release Lever.
        AutoCheckQuickReleaseLever = 41, "Auto Check: Quick Release Lever", paused: false;
        /// Auto Check: Door and Upper Cover.
        AutoCheckDoorAndUpperCover = 42, "Auto Check: Door and Upper Cover", paused: false;
        /// Laser Calibration.
        LaserCalibration = 43, "Laser Calibration", paused: false;
        /// Auto Check: Platform.
        AutoCheckPlatform = 44, "Auto Check: Platform", paused: false;
        /// Confirming BirdsEye Camera location.
        ConfirmingBirdsEyeCameraLocation = 45, "Confirming BirdsEye Camera location", paused: false;
        /// Calibrating BirdsEye Camera.
        CalibratingBirdsEyeCamera = 46, "Calibrating BirdsEye Camera", paused: false;
        /// Auto bed leveling - phase 1.
        AutoBedLevelingPhase1 = 47, "Auto bed leveling - phase 1", paused: false;
        /// Auto bed leveling - phase 2.
        AutoBedLevelingPhase2 = 48, "Auto bed leveling - phase 2", paused: false;
        /// Heating chamber.
        HeatingChamber = 49, "Heating chamber", paused: false;
        /// Adjusting heatbed temperature.
        AdjustingHeatbedTemperature = 50, "Adjusting heatbed temperature", paused: false;
        /// Printing calibration lines.
        PrintingCalibrationLines = 51, "Printing calibration lines", paused: false;
        /// Auto Check: Material.
        AutoCheckMaterial = 52, "Auto Check: Material", paused: false;
        /// Live View Camera Calibration.
        LiveViewCameraCalibration = 53, "Live View Camera Calibration", paused: false;
        /// Waiting for heatbed to reach target temperature.
        WaitingForHeatbedTemperature = 54, "Waiting for heatbed to reach target temperature", paused: false;
        /// Auto Check: Material Position.
        AutoCheckMaterialPosition = 55, "Auto Check: Material Position", paused: false;
        /// Cutting Module Offset Calibration.
        CuttingModuleOffsetCalibration = 56, "Cutting Module Offset Calibration", paused: false;
        /// Measuring Surface.
        MeasuringSurface = 57, "Measuring Surface", paused: false;
        /// Thermal Preconditioning for first layer optimization.
        ThermalPreconditioning = 58, "Thermal Preconditioning for first layer optimization", paused: false;
        /// Homing Blade Holder.
        HomingBladeHolder = 59, "Homing Blade Holder", paused: false;
        /// Calibrating Camera Offset.
        CalibratingCameraOffset = 60, "Calibrating Camera Offset", paused: false;
        /// Calibrating Blade Holder Position.
        CalibratingBladeHolderPosition = 61, "Calibrating Blade Holder Position", paused: false;
        /// Hotend Pick and Place Test.
        HotendPickAndPlaceTest = 62, "Hotend Pick and Place Test", paused: false;
        /// Waiting for the Chamber temperature to equalize.
        WaitingForChamberTemperatureEqualize = 63, "Waiting for the Chamber temperature to equalize", paused: false;
        /// Preparing Hotend.
        PreparingHotend = 64, "Preparing Hotend", paused: false;
        /// Calibrating the detection position of nozzle clumping.
        CalibratingNozzleClumpingDetection = 65, "Calibrating the detection position of nozzle clumping", paused: false;
        /// Purifying the chamber air.
        PurifyingChamberAir = 66, "Purifying the chamber air", paused: false;
        /// Measuring Rotary Attachment.
        MeasuringRotaryAttachment = 67, "Measuring Rotary Attachment", paused: false;
        /// The toolhead moves above the purge chute.
        ToolheadMovingAbovePurgeChute = 68, "The toolhead moves above the purge chute", paused: false;
        /// Cooling down the nozzle.
        CoolingNozzle = 69, "Cooling down the nozzle", paused: false;
        /// The toolhead moves to the center of the heatbed.
        ToolheadMovingToHeatbedCenter = 70, "The toolhead moves to the center of the heatbed", paused: false;
        /// Active Arc Fitting.
        ActiveArcFitting = 71, "Active Arc Fitting", paused: false;
        /// Hotend Type Detection.
        HotendTypeDetection = 72, "Hotend Type Detection", paused: false;
        /// Build plate alignment detection.
        BuildPlateAlignmentDetection = 73, "Build plate alignment detection", paused: false;
        /// Heatbed surface foreign object detection.
        HeatbedSurfaceForeignObjectDetection = 74, "Heatbed surface foreign object detection", paused: false;
        /// Heatbed underside foreign object detection.
        HeatbedUndersideForeignObjectDetection = 75, "Heatbed underside foreign object detection", paused: false;
        /// Pre-extrusion before printing.
        PreExtrusionBeforePrinting = 76, "Pre-extrusion before printing", paused: false;
        /// Preparing AMS.
        PreparingAms = 77, "Preparing AMS", paused: false;
    }
}

impl PrintStage {
    /// Returns true for the idle encodings (`-1` on X1, `255` on P1).
    ///
    /// Not a completion test: `stg_cur` reads idle for the tail of a calibration run that is
    /// still in progress. Check `gcode_state` for that.
    pub fn is_idle(self) -> bool {
        matches!(self, Self::Idle)
    }
}

/// Writes the [`label`](PrintStage::label), or `Unknown stage <id>` for an unrecognized id.
impl core::fmt::Display for PrintStage {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match (self.label(), self) {
            (Some(label), _) => f.write_str(label),
            (None, Self::Unknown(id)) => write!(f, "Unknown stage {id}"),
            (None, _) => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_stages_observed_on_hardware() {
        // P1S firmware 01.10.00.00, calibration captures.
        assert_eq!(PrintStage::from_wire(0), PrintStage::Printing);
        assert_eq!(PrintStage::from_wire(1), PrintStage::AutoBedLeveling);
        assert_eq!(PrintStage::from_wire(3), PrintStage::VibrationCompensation);
        assert_eq!(PrintStage::from_wire(14), PrintStage::CleaningNozzleTip);
        assert_eq!(
            PrintStage::from_wire(25),
            PrintStage::MotorNoiseCancellation
        );
    }

    #[test]
    fn both_idle_encodings_normalize() {
        // X1 sends -1, P1 sends 255; a consumer must not have to know which.
        assert_eq!(PrintStage::from_wire(-1), PrintStage::Idle);
        assert_eq!(PrintStage::from_wire(255), PrintStage::Idle);
        assert!(PrintStage::from_wire(-1).is_idle());
        assert!(PrintStage::from_wire(255).is_idle());
    }

    #[test]
    fn unknown_ids_round_trip_the_raw_value() {
        // Firmware adding a stage must not break decoding.
        assert_eq!(PrintStage::from_wire(200), PrintStage::Unknown(200));
        assert_eq!(PrintStage::from_wire(-7), PrintStage::Unknown(-7));
        assert!(PrintStage::from_wire(200).label().is_none());
    }

    #[test]
    fn every_documented_id_decodes_and_labels() {
        // 0..=77 is BambuStudio's full table; none may fall through to Unknown.
        for id in 0..=77 {
            let stage = PrintStage::from_wire(id);
            assert!(
                !matches!(stage, PrintStage::Unknown(_)),
                "id {id} fell through to Unknown"
            );
            assert!(stage.label().is_some(), "id {id} has no label");
        }
    }

    #[test]
    fn paused_states_are_classified() {
        assert!(PrintStage::from_wire(6).is_paused());
        assert!(PrintStage::from_wire(16).is_paused());
        assert!(PrintStage::from_wire(35).is_paused());
        assert!(!PrintStage::from_wire(0).is_paused());
        assert!(!PrintStage::from_wire(1).is_paused());
        assert!(!PrintStage::from_wire(255).is_paused());
    }

    #[test]
    fn duplicated_lidar_ids_share_a_label() {
        // Upstream marks 12 and 18 as duplicates; they stay distinct variants but read alike.
        assert_eq!(
            PrintStage::from_wire(12).label(),
            PrintStage::from_wire(18).label()
        );
    }

    #[test]
    fn display_is_the_label() {
        assert_eq!(
            PrintStage::from_wire(25).to_string(),
            "Motor noise cancellation"
        );
        assert_eq!(PrintStage::from_wire(255).to_string(), "Idle");
        assert_eq!(PrintStage::from_wire(999).to_string(), "Unknown stage 999");
    }
}
