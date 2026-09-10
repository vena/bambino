*[bambino](../../../index.md) / [types](../../index.md) / [telemetry](../index.md) / [stage](index.md)*

---

# Module `stage`

Typed decoding for the `stg_cur` / `stg` stage-ID space.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`PrintStage`](#printstage) | enum | A stage the printer reports in `stg_cur` (currently executing) or `stg` (queued). |

## Types

### `PrintStage`

```rust
enum PrintStage {
    Idle,
    Printing,
    AutoBedLeveling,
    HeatbedPreheating,
    VibrationCompensation,
    ChangingFilament,
    M400Pause,
    PausedFilamentRunout,
    HeatingNozzle,
    CalibratingDynamicFlow,
    ScanningBedSurface,
    InspectingFirstLayer,
    IdentifyingBuildPlateType,
    CalibratingMicroLidar,
    HomingToolhead,
    CleaningNozzleTip,
    CheckingExtruderTemperature,
    PausedByUser,
    PausedFrontCoverFallOff,
    CalibratingMicroLidarAlt,
    CalibratingFlowRatio,
    PausedNozzleTemperatureMalfunction,
    PausedHeatbedTemperatureMalfunction,
    FilamentUnloading,
    PausedStepLoss,
    FilamentLoading,
    MotorNoiseCancellation,
    PausedAmsOffline,
    PausedLowHeatbreakFanSpeed,
    PausedChamberTemperatureControlProblem,
    CoolingChamber,
    PausedUserGcode,
    MotorNoiseShowoff,
    PausedNozzleClumping,
    PausedCutterError,
    PausedFirstLayerError,
    PausedNozzleClog,
    MeasuringMotionPrecision,
    EnhancingMotionPrecision,
    MeasureMotionAccuracy,
    NozzleOffsetCalibration,
    HighTemperatureAutoBedLeveling,
    AutoCheckQuickReleaseLever,
    AutoCheckDoorAndUpperCover,
    LaserCalibration,
    AutoCheckPlatform,
    ConfirmingBirdsEyeCameraLocation,
    CalibratingBirdsEyeCamera,
    AutoBedLevelingPhase1,
    AutoBedLevelingPhase2,
    HeatingChamber,
    AdjustingHeatbedTemperature,
    PrintingCalibrationLines,
    AutoCheckMaterial,
    LiveViewCameraCalibration,
    WaitingForHeatbedTemperature,
    AutoCheckMaterialPosition,
    CuttingModuleOffsetCalibration,
    MeasuringSurface,
    ThermalPreconditioning,
    HomingBladeHolder,
    CalibratingCameraOffset,
    CalibratingBladeHolderPosition,
    HotendPickAndPlaceTest,
    WaitingForChamberTemperatureEqualize,
    PreparingHotend,
    CalibratingNozzleClumpingDetection,
    PurifyingChamberAir,
    MeasuringRotaryAttachment,
    ToolheadMovingAbovePurgeChute,
    CoolingNozzle,
    ToolheadMovingToHeatbedCenter,
    ActiveArcFitting,
    HotendTypeDetection,
    BuildPlateAlignmentDetection,
    HeatbedSurfaceForeignObjectDetection,
    HeatbedUndersideForeignObjectDetection,
    PreExtrusionBeforePrinting,
    PreparingAms,
    Unknown(i32),
}
```

A stage the printer reports in `stg_cur` (currently executing) or `stg` (queued).

The wire carries bare integers. [`PrintStage::from_wire`](#printstage) maps them to variants and leaves
anything unrecognized in [`PrintStage::Unknown`](#printstage) rather than failing, so a firmware that adds
a stage id does not break decoding.

# Read this before trusting a decoded value

Decoding does not make `stg_cur` trustworthy on its own. A1 and P1 firmware reports
`stg_cur = 0` ([`PrintStage::Printing`](#printstage)) while genuinely idle [REF-MQTT-IDLEBUG], so the value
means nothing unless `gcode_state` is `RUNNING` or `PAUSE`. Gate on that before displaying a
stage, or use a helper that does it for you — a typed [`PrintStage::Printing`](#printstage) handed to a UI
looks authoritative in a way the raw `0` did not, which makes ignoring the gate *more*
dangerous here, not less.

[`PrintStage::Idle`](#printstage) returning from a run in progress is also normal: once the last queued
stage finishes, `stg_cur` reads idle for the remainder of the run while `mc_percent` keeps
climbing. Completion is `gcode_state`/`mc_percent`, never this.

# Verification

Ids `0`–`77` come from BambuStudio's own `get_stage_string` (`DeviceManager.cpp`), the vendor
client's table. bambuddy's independent `STAGE_NAMES` corroborates 69 of the 78 and contradicts
none of them except id `74`, where bambuddy carries a self-described guess ("Preparing", noted
upstream as seen on H2D) and BambuStudio is taken as authoritative. Ids `67`–`73`, `75` and
`76` appear in BambuStudio alone.

Directly observed on hardware here (P1S, firmware `01.10.00.00`): `0`, `1`, `3`, `14`, `25`
and the `255` idle encoding. Everything else is upstream-sourced, not locally captured.

Idle is reported as `-1` on X1 and `255` on P1; both normalize to [`PrintStage::Idle`](#printstage), which
is the split a raw `i32` would otherwise leak to every consumer.

#### Variants

- **`Idle`**

  Idle. Wire sends `-1` on X1 and `255` on P1; both map here.

- **`Printing`**

  Printing.

- **`AutoBedLeveling`**

  Auto bed leveling.

- **`HeatbedPreheating`**

  Heatbed preheating.

- **`VibrationCompensation`**

  Resonance sweep. Upstream's machine name for this is `sweeping_xy_mech_mode`.

- **`ChangingFilament`**

  Changing filament.

- **`M400Pause`**

  M400 pause.

- **`PausedFilamentRunout`**

  Paused (filament ran out).

- **`HeatingNozzle`**

  Heating nozzle.

- **`CalibratingDynamicFlow`**

  Calibrating dynamic flow.

- **`ScanningBedSurface`**

  Scanning bed surface.

- **`InspectingFirstLayer`**

  Inspecting first layer.

- **`IdentifyingBuildPlateType`**

  Identifying build plate type.

- **`CalibratingMicroLidar`**

  Calibrating Micro Lidar.

- **`HomingToolhead`**

  Homing toolhead.

- **`CleaningNozzleTip`**

  Cleaning nozzle tip.

- **`CheckingExtruderTemperature`**

  Checking extruder temperature.

- **`PausedByUser`**

  Paused by the user.

- **`PausedFrontCoverFallOff`**

  Pause (front cover fall off).

- **`CalibratingMicroLidarAlt`**

  Second micro-lidar calibration id. Upstream marks `12` and `18` as duplicated.

- **`CalibratingFlowRatio`**

  Calibrating flow ratio.

- **`PausedNozzleTemperatureMalfunction`**

  Pause (nozzle temperature malfunction).

- **`PausedHeatbedTemperatureMalfunction`**

  Pause (heatbed temperature malfunction).

- **`FilamentUnloading`**

  Filament unloading.

- **`PausedStepLoss`**

  Pause (step loss).

- **`FilamentLoading`**

  Filament loading.

- **`MotorNoiseCancellation`**

  Motor noise cancellation.

- **`PausedAmsOffline`**

  Pause (AMS offline).

- **`PausedLowHeatbreakFanSpeed`**

  Pause (low speed of the heatbreak fan).

- **`PausedChamberTemperatureControlProblem`**

  Pause (chamber temperature control problem).

- **`CoolingChamber`**

  Cooling chamber.

- **`PausedUserGcode`**

  Pause (Gcode inserted by user).

- **`MotorNoiseShowoff`**

  Motor noise showoff.

- **`PausedNozzleClumping`**

  Pause (nozzle clumping).

- **`PausedCutterError`**

  Pause (cutter error).

- **`PausedFirstLayerError`**

  Pause (first layer error).

- **`PausedNozzleClog`**

  Pause (nozzle clog).

- **`MeasuringMotionPrecision`**

  Measuring motion precision.

- **`EnhancingMotionPrecision`**

  Enhancing motion precision.

- **`MeasureMotionAccuracy`**

  Measure motion accuracy.

- **`NozzleOffsetCalibration`**

  Nozzle offset calibration.

- **`HighTemperatureAutoBedLeveling`**

  High temperature auto bed leveling.

- **`AutoCheckQuickReleaseLever`**

  Auto Check: Quick Release Lever.

- **`AutoCheckDoorAndUpperCover`**

  Auto Check: Door and Upper Cover.

- **`LaserCalibration`**

  Laser Calibration.

- **`AutoCheckPlatform`**

  Auto Check: Platform.

- **`ConfirmingBirdsEyeCameraLocation`**

  Confirming BirdsEye Camera location.

- **`CalibratingBirdsEyeCamera`**

  Calibrating BirdsEye Camera.

- **`AutoBedLevelingPhase1`**

  Auto bed leveling - phase 1.

- **`AutoBedLevelingPhase2`**

  Auto bed leveling - phase 2.

- **`HeatingChamber`**

  Heating chamber.

- **`AdjustingHeatbedTemperature`**

  Adjusting heatbed temperature.

- **`PrintingCalibrationLines`**

  Printing calibration lines.

- **`AutoCheckMaterial`**

  Auto Check: Material.

- **`LiveViewCameraCalibration`**

  Live View Camera Calibration.

- **`WaitingForHeatbedTemperature`**

  Waiting for heatbed to reach target temperature.

- **`AutoCheckMaterialPosition`**

  Auto Check: Material Position.

- **`CuttingModuleOffsetCalibration`**

  Cutting Module Offset Calibration.

- **`MeasuringSurface`**

  Measuring Surface.

- **`ThermalPreconditioning`**

  Thermal Preconditioning for first layer optimization.

- **`HomingBladeHolder`**

  Homing Blade Holder.

- **`CalibratingCameraOffset`**

  Calibrating Camera Offset.

- **`CalibratingBladeHolderPosition`**

  Calibrating Blade Holder Position.

- **`HotendPickAndPlaceTest`**

  Hotend Pick and Place Test.

- **`WaitingForChamberTemperatureEqualize`**

  Waiting for the Chamber temperature to equalize.

- **`PreparingHotend`**

  Preparing Hotend.

- **`CalibratingNozzleClumpingDetection`**

  Calibrating the detection position of nozzle clumping.

- **`PurifyingChamberAir`**

  Purifying the chamber air.

- **`MeasuringRotaryAttachment`**

  Measuring Rotary Attachment.

- **`ToolheadMovingAbovePurgeChute`**

  The toolhead moves above the purge chute.

- **`CoolingNozzle`**

  Cooling down the nozzle.

- **`ToolheadMovingToHeatbedCenter`**

  The toolhead moves to the center of the heatbed.

- **`ActiveArcFitting`**

  Active Arc Fitting.

- **`HotendTypeDetection`**

  Hotend Type Detection.

- **`BuildPlateAlignmentDetection`**

  Build plate alignment detection.

- **`HeatbedSurfaceForeignObjectDetection`**

  Heatbed surface foreign object detection.

- **`HeatbedUndersideForeignObjectDetection`**

  Heatbed underside foreign object detection.

- **`PreExtrusionBeforePrinting`**

  Pre-extrusion before printing.

- **`PreparingAms`**

  Preparing AMS.

- **`Unknown`**

  A stage id no upstream table covers. Carries the raw wire value.

#### Implementations

- <span id="printstage-from-wire"></span>`fn from_wire(value: i32) -> Self`

  Decodes a raw `stg_cur` / `stg` wire value.

- <span id="printstage-is-idle"></span>`fn is_idle(self) -> bool`

  Returns true for the idle encodings (`-1` on X1, `255` on P1).

  Not a completion test: `stg_cur` reads idle for the tail of a calibration run that is
  still in progress. Check `gcode_state` for that.

- <span id="printstage-is-paused"></span>`fn is_paused(self) -> bool`

  Returns true if this stage is one of the paused states.

- <span id="printstage-label"></span>`fn label(self) -> Option<&'static str>`

  Human-readable label, matching BambuStudio's own wording.

  Returns `None` for [`PrintStage::Unknown`](#printstage) — the caller decides how to render an id no
  upstream table covers, rather than getting a fabricated label.

#### Trait Implementations

##### `impl Clone for PrintStage`

- <span id="printstage-clone"></span>`fn clone(&self) -> PrintStage` — [`PrintStage`](#printstage)

##### `impl Copy for PrintStage`

##### `impl Debug for PrintStage`

- <span id="printstage-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for PrintStage`

##### `impl Hash for PrintStage`

- <span id="printstage-hash"></span>`fn hash<__H: hash::Hasher>(&self, state: &mut __H)`

##### `impl PartialEq for PrintStage`

- <span id="printstage-partialeq-eq"></span>`fn eq(&self, other: &PrintStage) -> bool` — [`PrintStage`](#printstage)

