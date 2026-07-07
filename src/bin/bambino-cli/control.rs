#![cfg(feature = "cli")]

//! # Motion, Thermal, and Peripheral Control Subcommand
//!
//! Handles dispatching manual commands to the printer motion controller
//! and querying hardware modules from the expansion bus [REF-MOTO-GCODE].
//!
//! Incorporates detailed diagnostic telemetry printing if `--verbose` is enabled
//! to isolate connection, handshake, and packet serialization issues.

use std::io::{self, Write};
use std::time::Duration;

use bambino::client::{CalibrationOption, FanTarget, PrintSpeed};
use bambino::error::BambuError;
use bambino::mqtt::AirductMode;
use clap::{Subcommand, ValueEnum};

use crate::connection::create_printer;

#[derive(Clone, ValueEnum, Debug)]
pub enum FanTargetArg {
    Part,
    Aux,
    Exhaust,
    Right,
}

#[derive(Clone, ValueEnum, Debug)]
pub enum TempTargetArg {
    Nozzle,
    Bed,
    Chamber,
}

#[derive(Clone, ValueEnum, Debug)]
pub enum LedNodeArg {
    Chamber,
    Work,
}

#[derive(Clone, ValueEnum, Debug)]
pub enum LedStateArg {
    On,
    Off,
}

#[derive(Clone, ValueEnum, Debug)]
pub enum PrintSpeedArg {
    Silent,
    Standard,
    Sport,
    Ludicrous,
}

#[derive(Clone, ValueEnum, Debug)]
pub enum AirductModeArg {
    Cooling,
    Heating,
    Laser,
}

#[derive(Clone, ValueEnum, Debug)]
pub enum CalibrationArg {
    BedLeveling,
    Vibration,
    MotorNoise,
    NozzleHeight,
    HeatbedThermal,
}

#[derive(Subcommand, Debug)]
pub enum AmsAction {
    /// Start AMS drying cycle (time in minutes)
    Dry {
        id: i32,
        temp: u32,
        time: u32,
        #[arg(action = clap::ArgAction::Set, value_parser = clap::builder::BoolishValueParser::new())]
        rotate: bool,
        filament: String,
    },
    /// Stop AMS drying cycle
    DryStop { id: i32 },
}

#[derive(Subcommand, Debug)]
pub enum ControlAction {
    /// Home all structural motion axes safely
    Home,
    /// Execute relative motion (e.g., move z -10 3000)
    Move {
        axis: String,
        distance: f32,
        feedrate: Option<u32>,
    },
    /// Extrude relative filament length (e.g., extrude 10 900)
    Extrude { length: f32, feedrate: Option<u32> },
    /// Configure PWM fan speed
    Fan {
        target: FanTargetArg,
        speed_percent: u8,
    },
    /// Set hotend or build-plate temperatures
    Temp { target: TempTargetArg, value: u16 },
    /// Toggle chamber or auxiliary LEDs
    Led {
        node: LedNodeArg,
        state: LedStateArg,
    },
    /// Suspend print queue execution
    Pause,
    /// Resume print queue execution
    Resume,
    /// Abort active print job
    Stop,
    /// Send G-code with model safety checks
    Gcode { gcode_line: String },
    /// Send raw G-code bypassing safety checks
    GcodeRaw {
        /// Skip interactive confirmation prompt
        #[arg(long = "unsafe")]
        bypass_safety: bool,
        gcode_line: String,
    },
    /// Set print speed profile
    Speed { level: PrintSpeedArg },
    /// Clear active print error codes
    ClearError,
    /// Switch airduct damper mode
    Airduct { mode: AirductModeArg },
    /// Trigger one or more calibration routines
    Calibrate {
        #[arg(required = true)]
        routines: Vec<CalibrationArg>,
    },
    /// AMS filament management
    #[command(flatten_help = true)]
    Ams {
        #[command(subcommand)]
        action: AmsAction,
    },
}

/// Connects to the printer, sends a `get_version` command, and displays expansion bus modules.
pub async fn run_info(ip: &str, serial: &str, access_code: &str) -> Result<(), BambuError> {
    let is_verbose = crate::is_verbose();
    let mut printer = create_printer(ip, serial, access_code)?;

    println!("Querying expansion bus version database...");

    match tokio::time::timeout(Duration::from_secs(10), printer.get_version()).await {
        Ok(Ok(info)) => {
            let mut table = crate::table::Table::new(vec![
                "Product", "Module", "Hardware", "Firmware", "Serial",
            ]);

            for m in &info.module {
                if !m.visible && !is_verbose {
                    continue;
                }
                table.add_row(vec![&m.product_name, &m.name, &m.hw_ver, &m.sw_ver, &m.sn]);
            }

            println!();
            table.print();
            if !is_verbose {
                println!("\n  Use -v to show all internal modules.");
            }
            println!();
        }
        Ok(Err(e)) => {
            log::debug!("Version query generated an error: {:?}", e);
            return Err(e);
        }
        Err(_) => {
            println!("\n\x1B[1;33mNotice: Version query timed out after 10 seconds.\x1B[0m");
            println!(
                "Note: If this model does not reply, it confirms the physical firmware on this"
            );
            println!(
                "specific hardware track discards or ignores 'get_version' payloads over MQTTS.\n"
            );
        }
    }

    Ok(())
}

/// Prints `before_msg`, awaits `fut`, then prints `after_msg` on success — collapses the repeated "Dispatching.../call/...published successfully" triplet shared by most `ControlAction` match arms below.
async fn dispatch<T>(
    before_msg: &str,
    after_msg: &str,
    fut: impl std::future::Future<Output = Result<T, BambuError>>,
) -> Result<T, BambuError> {
    println!("{before_msg}");
    let result = fut.await?;
    println!("{after_msg}");
    Ok(result)
}

/// Dispatches a typed control action to the printer.
pub async fn run(
    ip: &str,
    serial: &str,
    access_code: &str,
    action: ControlAction,
) -> Result<(), BambuError> {
    log::debug!("Running control subcommand action: '{:?}'", action);

    let mut client = create_printer(ip, serial, access_code)?;

    match action {
        ControlAction::Home => {
            dispatch(
                "Dispatching safe homing command macro...",
                "Homing command published successfully.",
                client.home_axes(false),
            )
            .await?;
        }
        ControlAction::Move {
            axis,
            distance,
            feedrate,
        } => {
            if axis.len() != 1 {
                return Err(BambuError::ProtocolViolation(
                    format!(
                        "Invalid axis: '{}' (expected a single character X, Y, or Z)",
                        axis
                    )
                    .into(),
                ));
            }
            let axis_char = axis.chars().next().unwrap(); // safe: len() == 1 checked above
            let feedrate = feedrate.unwrap_or(3000);
            dispatch(
                "Dispatching motion G-code G0 relative move...",
                "Motion command published successfully.",
                client.move_relative(axis_char, distance, feedrate),
            )
            .await?;
        }
        ControlAction::Extrude { length, feedrate } => {
            let feedrate = feedrate.unwrap_or(900);
            dispatch(
                "Dispatching relative extrusion manual feed sequence...",
                "Extrusion command published successfully.",
                client.extrude(length, feedrate),
            )
            .await?;
        }
        ControlAction::Fan {
            target,
            speed_percent,
        } => {
            let fan_target = match target {
                FanTargetArg::Part => FanTarget::PartCooling,
                FanTargetArg::Aux => FanTarget::AuxiliaryLeft,
                FanTargetArg::Exhaust => FanTarget::ChamberExhaust,
                FanTargetArg::Right => FanTarget::AuxiliaryRight,
            };
            dispatch(
                "Configuring cooling fan PWM scale...",
                "Fan control command published successfully.",
                client.set_fan_speed(fan_target, speed_percent),
            )
            .await?;
        }
        ControlAction::Temp { target, value } => match target {
            TempTargetArg::Nozzle => {
                dispatch(
                    "Dispatching T0 hotend heater target...",
                    "Thermal command published successfully.",
                    client.set_nozzle_temperature(0, value),
                )
                .await?;
            }
            TempTargetArg::Bed => {
                dispatch(
                    "Dispatching build-plate heater target...",
                    "Thermal command published successfully.",
                    client.set_bed_temperature(value),
                )
                .await?;
            }
            TempTargetArg::Chamber => {
                dispatch(
                    "Dispatching chamber heating target...",
                    "Thermal command published successfully.",
                    client.set_chamber_temperature(value),
                )
                .await?;
            }
        },
        ControlAction::Led { node, state } => {
            let led_node = match node {
                LedNodeArg::Chamber => "chamber_light",
                LedNodeArg::Work => "work_light",
            };
            let turn_on = match state {
                LedStateArg::On => true,
                LedStateArg::Off => false,
            };
            dispatch(
                "Dispatching ledctrl command register block...",
                "LED command published successfully.",
                client.set_led(led_node, turn_on),
            )
            .await?;
        }
        ControlAction::Pause => {
            dispatch(
                "Suspending print queue execution...",
                "Pause command published successfully.",
                client.pause_print(),
            )
            .await?;
        }
        ControlAction::Resume => {
            dispatch(
                "Resuming print queue execution...",
                "Resume command published successfully.",
                client.resume_print(),
            )
            .await?;
        }
        ControlAction::Stop => {
            dispatch(
                "Aborting active print job pipeline...",
                "Stop command published successfully.",
                client.stop_print(),
            )
            .await?;
        }
        ControlAction::Gcode { gcode_line } => {
            dispatch(
                "Dispatching G-code (with safety checks)...",
                "G-code command published successfully.",
                client.send_gcode(&gcode_line),
            )
            .await?;
        }
        ControlAction::GcodeRaw {
            bypass_safety,
            gcode_line,
        } => {
            if !bypass_safety {
                eprint!(
                    "WARNING: gcode-raw bypasses all safety checks. \
                     Sending unsafe commands can damage your printer.\n\
                     Type 'yes' to confirm: "
                );
                io::stderr().flush().unwrap_or(());
                let mut confirmation = String::new();
                io::stdin().read_line(&mut confirmation).map_err(|_| {
                    BambuError::ProtocolViolation("Failed to read confirmation".into())
                })?;
                if confirmation.trim().to_lowercase() != "yes" {
                    println!("Aborted.");
                    return Ok(());
                }
            }
            println!("Dispatching raw G-code (no safety checks)...");
            client.send_gcode_raw(&gcode_line).await?;
            println!("Raw G-code command published successfully.");
        }
        ControlAction::Speed { level } => {
            let speed = match level {
                PrintSpeedArg::Silent => PrintSpeed::Silent,
                PrintSpeedArg::Standard => PrintSpeed::Standard,
                PrintSpeedArg::Sport => PrintSpeed::Sport,
                PrintSpeedArg::Ludicrous => PrintSpeed::Ludicrous,
            };
            dispatch(
                &format!("Setting print speed to {:?}...", level),
                "Print speed command published successfully.",
                client.set_print_speed(speed),
            )
            .await?;
        }
        ControlAction::ClearError => {
            dispatch(
                "Clearing active print error codes...",
                "Clear error command published successfully.",
                client.clear_print_error(),
            )
            .await?;
        }
        ControlAction::Airduct { mode } => {
            let airduct_mode = match mode {
                AirductModeArg::Cooling => AirductMode::Cooling,
                AirductModeArg::Heating => AirductMode::Heating,
                AirductModeArg::Laser => AirductMode::Laser,
            };
            dispatch(
                &format!("Switching airduct damper to {:?} mode...", mode),
                "Airduct command published successfully.",
                client.set_airduct_mode(airduct_mode),
            )
            .await?;
        }
        ControlAction::Calibrate { routines } => {
            let mut options = CalibrationOption(0);
            for routine in routines {
                let flag = match routine {
                    CalibrationArg::BedLeveling => CalibrationOption::BED_LEVELING,
                    CalibrationArg::Vibration => CalibrationOption::VIBRATION_COMPENSATION,
                    CalibrationArg::MotorNoise => CalibrationOption::MOTOR_NOISE_CANCELLATION,
                    CalibrationArg::NozzleHeight => CalibrationOption::NOZZLE_HEIGHT,
                    CalibrationArg::HeatbedThermal => CalibrationOption::HEATBED_THERMAL,
                };
                options = options | flag;
            }
            println!("Triggering calibration routines...");
            client.start_calibration(options).await?;
            println!("Calibration command published successfully.");
        }
        ControlAction::Ams { action } => match action {
            AmsAction::Dry {
                id,
                temp,
                time,
                rotate,
                filament,
            } => {
                dispatch(
                    &format!(
                        "Starting AMS {} drying cycle at {}°C for {} minutes...",
                        id, temp, time
                    ),
                    "AMS drying command published successfully.",
                    client.start_drying(id, temp, time, rotate, &filament),
                )
                .await?;
            }
            AmsAction::DryStop { id } => {
                dispatch(
                    &format!("Stopping AMS {} drying cycle...", id),
                    "AMS drying stop command published successfully.",
                    client.stop_drying(id),
                )
                .await?;
            }
        },
    }

    Ok(())
}
