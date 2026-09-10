//! Typed decoding for the `stg_cur` / `stg` stage-ID space.

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PrintStage {
    /// Idle. Wire sends `-1` on X1 and `255` on P1; both map here.
    Idle,
    /// Printing.
    Printing,
    /// Auto bed leveling.
    AutoBedLeveling,
    /// Heatbed preheating.
    HeatbedPreheating,
    /// Resonance sweep. Upstream's machine name for this is `sweeping_xy_mech_mode`.
    VibrationCompensation,
    /// Changing filament.
    ChangingFilament,
    /// M400 pause.
    M400Pause,
    /// Paused (filament ran out).
    PausedFilamentRunout,
    /// Heating nozzle.
    HeatingNozzle,
    /// Calibrating dynamic flow.
    CalibratingDynamicFlow,
    /// Scanning bed surface.
    ScanningBedSurface,
    /// Inspecting first layer.
    InspectingFirstLayer,
    /// Identifying build plate type.
    IdentifyingBuildPlateType,
    /// Calibrating Micro Lidar.
    CalibratingMicroLidar,
    /// Homing toolhead.
    HomingToolhead,
    /// Cleaning nozzle tip.
    CleaningNozzleTip,
    /// Checking extruder temperature.
    CheckingExtruderTemperature,
    /// Paused by the user.
    PausedByUser,
    /// Pause (front cover fall off).
    PausedFrontCoverFallOff,
    /// Second micro-lidar calibration id. Upstream marks `12` and `18` as duplicated.
    CalibratingMicroLidarAlt,
    /// Calibrating flow ratio.
    CalibratingFlowRatio,
    /// Pause (nozzle temperature malfunction).
    PausedNozzleTemperatureMalfunction,
    /// Pause (heatbed temperature malfunction).
    PausedHeatbedTemperatureMalfunction,
    /// Filament unloading.
    FilamentUnloading,
    /// Pause (step loss).
    PausedStepLoss,
    /// Filament loading.
    FilamentLoading,
    /// Motor noise cancellation.
    MotorNoiseCancellation,
    /// Pause (AMS offline).
    PausedAmsOffline,
    /// Pause (low speed of the heatbreak fan).
    PausedLowHeatbreakFanSpeed,
    /// Pause (chamber temperature control problem).
    PausedChamberTemperatureControlProblem,
    /// Cooling chamber.
    CoolingChamber,
    /// Pause (Gcode inserted by user).
    PausedUserGcode,
    /// Motor noise showoff.
    MotorNoiseShowoff,
    /// Pause (nozzle clumping).
    PausedNozzleClumping,
    /// Pause (cutter error).
    PausedCutterError,
    /// Pause (first layer error).
    PausedFirstLayerError,
    /// Pause (nozzle clog).
    PausedNozzleClog,
    /// Measuring motion precision.
    MeasuringMotionPrecision,
    /// Enhancing motion precision.
    EnhancingMotionPrecision,
    /// Measure motion accuracy.
    MeasureMotionAccuracy,
    /// Nozzle offset calibration.
    NozzleOffsetCalibration,
    /// High temperature auto bed leveling.
    HighTemperatureAutoBedLeveling,
    /// Auto Check: Quick Release Lever.
    AutoCheckQuickReleaseLever,
    /// Auto Check: Door and Upper Cover.
    AutoCheckDoorAndUpperCover,
    /// Laser Calibration.
    LaserCalibration,
    /// Auto Check: Platform.
    AutoCheckPlatform,
    /// Confirming BirdsEye Camera location.
    ConfirmingBirdsEyeCameraLocation,
    /// Calibrating BirdsEye Camera.
    CalibratingBirdsEyeCamera,
    /// Auto bed leveling - phase 1.
    AutoBedLevelingPhase1,
    /// Auto bed leveling - phase 2.
    AutoBedLevelingPhase2,
    /// Heating chamber.
    HeatingChamber,
    /// Adjusting heatbed temperature.
    AdjustingHeatbedTemperature,
    /// Printing calibration lines.
    PrintingCalibrationLines,
    /// Auto Check: Material.
    AutoCheckMaterial,
    /// Live View Camera Calibration.
    LiveViewCameraCalibration,
    /// Waiting for heatbed to reach target temperature.
    WaitingForHeatbedTemperature,
    /// Auto Check: Material Position.
    AutoCheckMaterialPosition,
    /// Cutting Module Offset Calibration.
    CuttingModuleOffsetCalibration,
    /// Measuring Surface.
    MeasuringSurface,
    /// Thermal Preconditioning for first layer optimization.
    ThermalPreconditioning,
    /// Homing Blade Holder.
    HomingBladeHolder,
    /// Calibrating Camera Offset.
    CalibratingCameraOffset,
    /// Calibrating Blade Holder Position.
    CalibratingBladeHolderPosition,
    /// Hotend Pick and Place Test.
    HotendPickAndPlaceTest,
    /// Waiting for the Chamber temperature to equalize.
    WaitingForChamberTemperatureEqualize,
    /// Preparing Hotend.
    PreparingHotend,
    /// Calibrating the detection position of nozzle clumping.
    CalibratingNozzleClumpingDetection,
    /// Purifying the chamber air.
    PurifyingChamberAir,
    /// Measuring Rotary Attachment.
    MeasuringRotaryAttachment,
    /// The toolhead moves above the purge chute.
    ToolheadMovingAbovePurgeChute,
    /// Cooling down the nozzle.
    CoolingNozzle,
    /// The toolhead moves to the center of the heatbed.
    ToolheadMovingToHeatbedCenter,
    /// Active Arc Fitting.
    ActiveArcFitting,
    /// Hotend Type Detection.
    HotendTypeDetection,
    /// Build plate alignment detection.
    BuildPlateAlignmentDetection,
    /// Heatbed surface foreign object detection.
    HeatbedSurfaceForeignObjectDetection,
    /// Heatbed underside foreign object detection.
    HeatbedUndersideForeignObjectDetection,
    /// Pre-extrusion before printing.
    PreExtrusionBeforePrinting,
    /// Preparing AMS.
    PreparingAms,
    /// A stage id no upstream table covers. Carries the raw wire value.
    Unknown(i32),
}

impl PrintStage {
    /// Decodes a raw `stg_cur` / `stg` wire value.
    pub fn from_wire(value: i32) -> Self {
        match value {
            -1 | 255 => Self::Idle,
            0 => Self::Printing,
            1 => Self::AutoBedLeveling,
            2 => Self::HeatbedPreheating,
            3 => Self::VibrationCompensation,
            4 => Self::ChangingFilament,
            5 => Self::M400Pause,
            6 => Self::PausedFilamentRunout,
            7 => Self::HeatingNozzle,
            8 => Self::CalibratingDynamicFlow,
            9 => Self::ScanningBedSurface,
            10 => Self::InspectingFirstLayer,
            11 => Self::IdentifyingBuildPlateType,
            12 => Self::CalibratingMicroLidar,
            13 => Self::HomingToolhead,
            14 => Self::CleaningNozzleTip,
            15 => Self::CheckingExtruderTemperature,
            16 => Self::PausedByUser,
            17 => Self::PausedFrontCoverFallOff,
            18 => Self::CalibratingMicroLidarAlt,
            19 => Self::CalibratingFlowRatio,
            20 => Self::PausedNozzleTemperatureMalfunction,
            21 => Self::PausedHeatbedTemperatureMalfunction,
            22 => Self::FilamentUnloading,
            23 => Self::PausedStepLoss,
            24 => Self::FilamentLoading,
            25 => Self::MotorNoiseCancellation,
            26 => Self::PausedAmsOffline,
            27 => Self::PausedLowHeatbreakFanSpeed,
            28 => Self::PausedChamberTemperatureControlProblem,
            29 => Self::CoolingChamber,
            30 => Self::PausedUserGcode,
            31 => Self::MotorNoiseShowoff,
            32 => Self::PausedNozzleClumping,
            33 => Self::PausedCutterError,
            34 => Self::PausedFirstLayerError,
            35 => Self::PausedNozzleClog,
            36 => Self::MeasuringMotionPrecision,
            37 => Self::EnhancingMotionPrecision,
            38 => Self::MeasureMotionAccuracy,
            39 => Self::NozzleOffsetCalibration,
            40 => Self::HighTemperatureAutoBedLeveling,
            41 => Self::AutoCheckQuickReleaseLever,
            42 => Self::AutoCheckDoorAndUpperCover,
            43 => Self::LaserCalibration,
            44 => Self::AutoCheckPlatform,
            45 => Self::ConfirmingBirdsEyeCameraLocation,
            46 => Self::CalibratingBirdsEyeCamera,
            47 => Self::AutoBedLevelingPhase1,
            48 => Self::AutoBedLevelingPhase2,
            49 => Self::HeatingChamber,
            50 => Self::AdjustingHeatbedTemperature,
            51 => Self::PrintingCalibrationLines,
            52 => Self::AutoCheckMaterial,
            53 => Self::LiveViewCameraCalibration,
            54 => Self::WaitingForHeatbedTemperature,
            55 => Self::AutoCheckMaterialPosition,
            56 => Self::CuttingModuleOffsetCalibration,
            57 => Self::MeasuringSurface,
            58 => Self::ThermalPreconditioning,
            59 => Self::HomingBladeHolder,
            60 => Self::CalibratingCameraOffset,
            61 => Self::CalibratingBladeHolderPosition,
            62 => Self::HotendPickAndPlaceTest,
            63 => Self::WaitingForChamberTemperatureEqualize,
            64 => Self::PreparingHotend,
            65 => Self::CalibratingNozzleClumpingDetection,
            66 => Self::PurifyingChamberAir,
            67 => Self::MeasuringRotaryAttachment,
            68 => Self::ToolheadMovingAbovePurgeChute,
            69 => Self::CoolingNozzle,
            70 => Self::ToolheadMovingToHeatbedCenter,
            71 => Self::ActiveArcFitting,
            72 => Self::HotendTypeDetection,
            73 => Self::BuildPlateAlignmentDetection,
            74 => Self::HeatbedSurfaceForeignObjectDetection,
            75 => Self::HeatbedUndersideForeignObjectDetection,
            76 => Self::PreExtrusionBeforePrinting,
            77 => Self::PreparingAms,
            other => Self::Unknown(other),
        }
    }

    /// Returns true for the idle encodings (`-1` on X1, `255` on P1).
    ///
    /// Not a completion test: `stg_cur` reads idle for the tail of a calibration run that is
    /// still in progress. Check `gcode_state` for that.
    pub fn is_idle(self) -> bool {
        matches!(self, Self::Idle)
    }

    /// Returns true if this stage is one of the paused states.
    pub fn is_paused(self) -> bool {
        matches!(
            self,
            Self::M400Pause
                | Self::PausedFilamentRunout
                | Self::PausedByUser
                | Self::PausedFrontCoverFallOff
                | Self::PausedNozzleTemperatureMalfunction
                | Self::PausedHeatbedTemperatureMalfunction
                | Self::PausedStepLoss
                | Self::PausedAmsOffline
                | Self::PausedLowHeatbreakFanSpeed
                | Self::PausedChamberTemperatureControlProblem
                | Self::PausedUserGcode
                | Self::PausedNozzleClumping
                | Self::PausedCutterError
                | Self::PausedFirstLayerError
                | Self::PausedNozzleClog
        )
    }

    /// Human-readable label, matching BambuStudio's own wording.
    ///
    /// Returns `None` for [`PrintStage::Unknown`] — the caller decides how to render an id no
    /// upstream table covers, rather than getting a fabricated label.
    pub fn label(self) -> Option<&'static str> {
        let s = match self {
            Self::Idle => "Idle",
            Self::Printing => "Printing",
            Self::AutoBedLeveling => "Auto bed leveling",
            Self::HeatbedPreheating => "Heatbed preheating",
            Self::VibrationCompensation => "Vibration compensation",
            Self::ChangingFilament => "Changing filament",
            Self::M400Pause => "M400 pause",
            Self::PausedFilamentRunout => "Paused (filament ran out)",
            Self::HeatingNozzle => "Heating nozzle",
            Self::CalibratingDynamicFlow => "Calibrating dynamic flow",
            Self::ScanningBedSurface => "Scanning bed surface",
            Self::InspectingFirstLayer => "Inspecting first layer",
            Self::IdentifyingBuildPlateType => "Identifying build plate type",
            Self::CalibratingMicroLidar | Self::CalibratingMicroLidarAlt => {
                "Calibrating Micro Lidar"
            }
            Self::HomingToolhead => "Homing toolhead",
            Self::CleaningNozzleTip => "Cleaning nozzle tip",
            Self::CheckingExtruderTemperature => "Checking extruder temperature",
            Self::PausedByUser => "Paused by the user",
            Self::PausedFrontCoverFallOff => "Pause (front cover fall off)",
            Self::CalibratingFlowRatio => "Calibrating flow ratio",
            Self::PausedNozzleTemperatureMalfunction => "Pause (nozzle temperature malfunction)",
            Self::PausedHeatbedTemperatureMalfunction => "Pause (heatbed temperature malfunction)",
            Self::FilamentUnloading => "Filament unloading",
            Self::PausedStepLoss => "Pause (step loss)",
            Self::FilamentLoading => "Filament loading",
            Self::MotorNoiseCancellation => "Motor noise cancellation",
            Self::PausedAmsOffline => "Pause (AMS offline)",
            Self::PausedLowHeatbreakFanSpeed => "Pause (low speed of the heatbreak fan)",
            Self::PausedChamberTemperatureControlProblem => {
                "Pause (chamber temperature control problem)"
            }
            Self::CoolingChamber => "Cooling chamber",
            Self::PausedUserGcode => "Pause (Gcode inserted by user)",
            Self::MotorNoiseShowoff => "Motor noise showoff",
            Self::PausedNozzleClumping => "Pause (nozzle clumping)",
            Self::PausedCutterError => "Pause (cutter error)",
            Self::PausedFirstLayerError => "Pause (first layer error)",
            Self::PausedNozzleClog => "Pause (nozzle clog)",
            Self::MeasuringMotionPrecision => "Measuring motion precision",
            Self::EnhancingMotionPrecision => "Enhancing motion precision",
            Self::MeasureMotionAccuracy => "Measure motion accuracy",
            Self::NozzleOffsetCalibration => "Nozzle offset calibration",
            Self::HighTemperatureAutoBedLeveling => "High temperature auto bed leveling",
            Self::AutoCheckQuickReleaseLever => "Auto Check: Quick Release Lever",
            Self::AutoCheckDoorAndUpperCover => "Auto Check: Door and Upper Cover",
            Self::LaserCalibration => "Laser Calibration",
            Self::AutoCheckPlatform => "Auto Check: Platform",
            Self::ConfirmingBirdsEyeCameraLocation => "Confirming BirdsEye Camera location",
            Self::CalibratingBirdsEyeCamera => "Calibrating BirdsEye Camera",
            Self::AutoBedLevelingPhase1 => "Auto bed leveling - phase 1",
            Self::AutoBedLevelingPhase2 => "Auto bed leveling - phase 2",
            Self::HeatingChamber => "Heating chamber",
            Self::AdjustingHeatbedTemperature => "Adjusting heatbed temperature",
            Self::PrintingCalibrationLines => "Printing calibration lines",
            Self::AutoCheckMaterial => "Auto Check: Material",
            Self::LiveViewCameraCalibration => "Live View Camera Calibration",
            Self::WaitingForHeatbedTemperature => "Waiting for heatbed to reach target temperature",
            Self::AutoCheckMaterialPosition => "Auto Check: Material Position",
            Self::CuttingModuleOffsetCalibration => "Cutting Module Offset Calibration",
            Self::MeasuringSurface => "Measuring Surface",
            Self::ThermalPreconditioning => "Thermal Preconditioning for first layer optimization",
            Self::HomingBladeHolder => "Homing Blade Holder",
            Self::CalibratingCameraOffset => "Calibrating Camera Offset",
            Self::CalibratingBladeHolderPosition => "Calibrating Blade Holder Position",
            Self::HotendPickAndPlaceTest => "Hotend Pick and Place Test",
            Self::WaitingForChamberTemperatureEqualize => {
                "Waiting for the Chamber temperature to equalize"
            }
            Self::PreparingHotend => "Preparing Hotend",
            Self::CalibratingNozzleClumpingDetection => {
                "Calibrating the detection position of nozzle clumping"
            }
            Self::PurifyingChamberAir => "Purifying the chamber air",
            Self::MeasuringRotaryAttachment => "Measuring Rotary Attachment",
            Self::ToolheadMovingAbovePurgeChute => "The toolhead moves above the purge chute",
            Self::CoolingNozzle => "Cooling down the nozzle",
            Self::ToolheadMovingToHeatbedCenter => {
                "The toolhead moves to the center of the heatbed"
            }
            Self::ActiveArcFitting => "Active Arc Fitting",
            Self::HotendTypeDetection => "Hotend Type Detection",
            Self::BuildPlateAlignmentDetection => "Build plate alignment detection",
            Self::HeatbedSurfaceForeignObjectDetection => {
                "Heatbed surface foreign object detection"
            }
            Self::HeatbedUndersideForeignObjectDetection => {
                "Heatbed underside foreign object detection"
            }
            Self::PreExtrusionBeforePrinting => "Pre-extrusion before printing",
            Self::PreparingAms => "Preparing AMS",
            Self::Unknown(_) => return None,
        };
        Some(s)
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
}
