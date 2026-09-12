#![cfg(feature = "cli")]

//! # Motion, Thermal, and Peripheral Control Subcommand
//!
//! Handles dispatching manual commands to the printer motion controller
//! and querying hardware modules from the expansion bus [REF-MOTO-GCODE].
//!
//! Incorporates detailed diagnostic telemetry printing if `--verbose` is enabled
//! to isolate connection, handshake, and packet serialization issues.

use std::io::{self, Write};
use std::time::{Duration, Instant};

use bambino::Error;
use bambino::client::{CalibrationOption, FanTarget, PrintSpeed};
use bambino::mqtt::AirductMode;
use bambino::types::DryingMaterial;
use bambino::types::telemetry::AmsUnitModel;
use clap::{Subcommand, ValueEnum};

use crate::error::CliError;

use crate::connection::{Printer, create_printer};

#[derive(Clone, ValueEnum, Debug)]
pub enum FanTargetArg {
    Part,
    Aux,
    Exhaust,
    Left2,
}

#[derive(Clone, Copy, ValueEnum, Debug)]
pub enum AxisArg {
    X,
    Y,
    Z,
}

impl AxisArg {
    fn as_char(self) -> char {
        match self {
            AxisArg::X => 'X',
            AxisArg::Y => 'Y',
            AxisArg::Z => 'Z',
        }
    }
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
    /// Start AMS drying cycle (duration is in hours, not minutes)
    ///
    /// Give `--material` to use Bambu's own published parameters for that filament, or set
    /// `--temp` and `--duration-hours` yourself. Explicit flags override the material.
    #[command(
        override_usage = "bambino-cli control <IP> <SERIAL> [ACCESS_CODE] ams dry <ID> --material <NAME> | --temp <C> --duration-hours <H>"
    )]
    Dry {
        id: i32,
        /// Filament material (PLA, PETG, ABS, PA-CF, ...). Fills temperature, duration and
        /// cooling temperature from Bambu's published drying parameters for the attached unit.
        #[arg(long)]
        material: Option<String>,
        /// Drying temperature in °C. Overrides --material.
        #[arg(long)]
        temp: Option<u32>,
        /// Cycle duration in whole hours. Overrides --material.
        #[arg(long)]
        duration_hours: Option<u32>,
        /// Filament type string sent on the wire. Defaults to --material's name; set this for a
        /// material the table does not name.
        #[arg(long)]
        filament: Option<String>,
        /// Rotate trays during the cycle
        #[arg(long, default_value_t = false)]
        rotate: bool,
        /// Target humidity (0 = firmware default)
        #[arg(long)]
        humidity: Option<u32>,
        /// Cooling temperature. Defaults to the material's softening temperature, else 50 —
        /// BambuStudio's own fallback. Pass explicitly to override.
        #[arg(long)]
        cooling_temp: Option<i32>,
        /// Override the AMS unit's power-conflict interlock
        #[arg(long, default_value_t = false)]
        close_power_conflict: bool,
    },
    /// Stop AMS drying cycle
    DryStop { id: i32 },
}

#[derive(Subcommand, Debug)]
pub enum ControlAction {
    /// Home all structural motion axes safely
    #[command(override_usage = "bambino-cli control <IP> <SERIAL> [ACCESS_CODE] home")]
    Home,
    /// Execute relative motion (e.g., move z -10 3000)
    #[command(
        override_usage = "bambino-cli control <IP> <SERIAL> [ACCESS_CODE] move <AXIS> <DISTANCE> [FEEDRATE]"
    )]
    Move {
        axis: AxisArg,
        distance: f32,
        feedrate: Option<u32>,
    },
    /// Extrude relative filament length (e.g., extrude 10 900)
    #[command(
        override_usage = "bambino-cli control <IP> <SERIAL> [ACCESS_CODE] extrude <LENGTH> [FEEDRATE]"
    )]
    Extrude { length: f32, feedrate: Option<u32> },
    /// Configure PWM fan speed
    #[command(
        override_usage = "bambino-cli control <IP> <SERIAL> [ACCESS_CODE] fan <TARGET> <SPEED_PERCENT>"
    )]
    Fan {
        target: FanTargetArg,
        speed_percent: u8,
    },
    /// Set hotend or build-plate temperatures
    #[command(
        override_usage = "bambino-cli control <IP> <SERIAL> [ACCESS_CODE] temp <TARGET> <VALUE>"
    )]
    Temp { target: TempTargetArg, value: u16 },
    /// Toggle chamber or auxiliary LEDs
    #[command(
        override_usage = "bambino-cli control <IP> <SERIAL> [ACCESS_CODE] led <NODE> <STATE>"
    )]
    Led {
        node: LedNodeArg,
        state: LedStateArg,
    },
    /// Suspend print queue execution
    #[command(override_usage = "bambino-cli control <IP> <SERIAL> [ACCESS_CODE] pause")]
    Pause,
    /// Resume print queue execution
    #[command(override_usage = "bambino-cli control <IP> <SERIAL> [ACCESS_CODE] resume")]
    Resume,
    /// Abort active print job
    #[command(override_usage = "bambino-cli control <IP> <SERIAL> [ACCESS_CODE] stop")]
    Stop,
    /// Send G-code with model safety checks
    #[command(
        override_usage = "bambino-cli control <IP> <SERIAL> [ACCESS_CODE] gcode <GCODE_LINE>"
    )]
    Gcode { gcode_line: String },
    /// Send raw G-code bypassing safety checks
    #[command(
        override_usage = "bambino-cli control <IP> <SERIAL> [ACCESS_CODE] gcode-raw [OPTIONS] <GCODE_LINE>"
    )]
    GcodeRaw {
        /// Skip interactive confirmation prompt
        #[arg(long = "unsafe")]
        bypass_safety: bool,
        gcode_line: String,
    },
    /// Set print speed profile
    #[command(override_usage = "bambino-cli control <IP> <SERIAL> [ACCESS_CODE] speed <LEVEL>")]
    Speed { level: PrintSpeedArg },
    /// Clear active print error codes
    #[command(override_usage = "bambino-cli control <IP> <SERIAL> [ACCESS_CODE] clear-error")]
    ClearError,
    /// Switch airduct damper mode
    #[command(override_usage = "bambino-cli control <IP> <SERIAL> [ACCESS_CODE] airduct <MODE>")]
    Airduct { mode: AirductModeArg },
    /// Trigger one or more calibration routines
    #[command(
        override_usage = "bambino-cli control <IP> <SERIAL> [ACCESS_CODE] calibrate <ROUTINES>... [--watch]"
    )]
    Calibrate {
        #[arg(required = true)]
        routines: Vec<CalibrationArg>,
        /// Stay connected after publishing and stream every subsequent `print`-bearing push as
        /// one compact NDJSON line until interrupted (Ctrl+C), instead of exiting immediately.
        ///
        /// Same output shape as `dump --follow`, but subscribed before the command is published
        /// — running `dump --follow` in a second terminal races the trigger and can miss the
        /// first pushes of the run. See issue #227: whether a standalone calibration reports
        /// any progress at all is unverified, and this flag exists to capture the evidence.
        #[arg(short = 'w', long)]
        watch: bool,
    },
    /// AMS filament management
    #[command(flatten_help = true)]
    Ams {
        #[command(subcommand)]
        action: AmsAction,
    },
}

/// Connects to the printer, sends a `get_version` command, and displays expansion bus modules.
///
/// Module serials identify physical hardware, so they're omitted by default (the `Serial`
/// column doesn't appear at all — a placeholder would just be a column of noise). Pass
/// `show_serials` to print the real values, on stdout, as an explicit opt-in: that's what
/// makes a subsequent redirect the operator's own decision rather than a surprise.
pub async fn run_info(
    ip: &str,
    serial: &str,
    access_code: &str,
    show_serials: bool,
) -> Result<(), CliError> {
    let is_verbose = crate::is_verbose();
    let mut printer = create_printer(ip, serial, access_code)?;

    println!("Querying expansion bus version database...");

    match tokio::time::timeout(Duration::from_secs(10), printer.get_version()).await {
        Ok(Ok(info)) => {
            let mut headers = vec!["Product", "Module", "Hardware", "Firmware"];
            if show_serials {
                headers.push("Serial");
            }
            let mut table = crate::table::Table::new(headers);

            for m in &info.module {
                if !m.visible && !is_verbose {
                    continue;
                }
                let mut row = vec![
                    m.product_name.as_str(),
                    m.name.as_str(),
                    m.hw_ver.as_str(),
                    m.sw_ver.as_str(),
                ];
                if show_serials {
                    row.push(m.sn.as_str());
                }
                table.add_row(row);
            }

            println!();
            table.print();
            if !is_verbose {
                println!("\n  Use -v to show all internal modules.");
            }
            if !show_serials {
                println!("  Use --show-serials to include module serial numbers.");
            }
            println!();
        }
        Ok(Err(e)) => {
            log::debug!("Version query generated an error: {:?}", e);
            return Err(e.into());
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
    fut: impl std::future::Future<Output = Result<T, Error>>,
) -> Result<T, Error> {
    println!("{before_msg}");
    let result = fut.await?;
    println!("{after_msg}");
    Ok(result)
}

/// Parsed `ams dry` flags, passed as one struct so `run()` stays readable.
struct DryArgs {
    id: i32,
    material: Option<String>,
    temp: Option<u32>,
    duration_hours: Option<u32>,
    rotate: bool,
    filament: Option<String>,
    humidity: Option<u32>,
    cooling_temp: Option<i32>,
    close_power_conflict: bool,
}

/// How long [`resolve_dry_unit`] waits for telemetry that actually names the AMS unit before
/// giving up and warning. Sized to cover a pushall round trip on a busy printer without
/// stalling an interactive command; the fallback is still a sound answer if it expires.
const DRY_UNIT_RESOLVE_TIMEOUT_SECS: u64 = 5;

/// Reads the attached unit type for `ams_id` out of the cached telemetry snapshot.
///
/// Telemetry stores an A2L-attached AMS Lite under its normalized id 6, so a physical 16 is
/// normalized before the lookup.
fn cached_dry_unit(client: &Printer, ams_id: i32) -> Option<AmsUnitModel> {
    let ams_id = u8::try_from(ams_id).map_or(ams_id, |id| {
        i32::from(bambino::ams::normalize_ams_unit_id(id))
    });
    client.ams().and_then(|ams| {
        ams.ams
            .iter()
            .find(|u| u.id.parse::<i32>() == Ok(ams_id))
            .and_then(|u| u.unit_model())
    })
}

/// Resolves the attached AMS unit so a material's parameters can be read from the right column.
///
/// Bambu publishes different values per unit type — PA is 65 °C on an AMS 2 Pro and 85 °C on an
/// AMS-HT — so the material cannot be applied without knowing which is plugged in.
///
/// Requests a full state dump and then polls until the AMS array actually arrives, rather than
/// reading whatever a single poll happens to return. Automatic broadcasts carry only fields that
/// changed since the last transmission [REF-MQTT-TELEMETRY], so on a printer whose AMS state has
/// been static the `ams.ams` array may not appear in any incremental frame at all — one poll
/// routinely lands on a temperature-only delta and leaves the cache empty. `probe.rs`'s homing
/// warm-up loop exists for the same reason.
///
/// Falls back to the AMS 2 Pro column when the unit cannot be identified, matching bambuddy
/// (`print_scheduler.py:3807`, `temp_key = module_type if module_type in ("n3f","n3s") else
/// "n3f"`). That is also the lower of the two columns on every material where they differ, so an
/// unidentified unit errs cool rather than hot — and the real ceiling is enforced by the drying
/// gate regardless. The fallback now warns on stderr: erring cool still means the filament
/// under-dries, and silently substituting a guessed column is the failure this exists to make
/// visible.
async fn resolve_dry_unit(client: &mut Printer, ams_id: i32) -> AmsUnitModel {
    // Neither the pushall nor a failed poll is fatal: the fallback below is a sound answer, and
    // the drying gate still refuses anything the hardware would reject.
    let _ = client.request_pushall().await;
    let deadline = Instant::now() + Duration::from_secs(DRY_UNIT_RESOLVE_TIMEOUT_SECS);
    loop {
        if let Some(model) = cached_dry_unit(client, ams_id) {
            return model;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        if !matches!(
            tokio::time::timeout(remaining, client.poll_telemetry()).await,
            Ok(Ok(_))
        ) {
            break;
        }
    }
    eprintln!(
        "warning: AMS {ams_id} never reported its unit type within {DRY_UNIT_RESOLVE_TIMEOUT_SECS}s — \
         using the AMS 2 Pro material column. An AMS-HT dries this material hotter, so the \
         filament may be under-dried; pass --temp explicitly to override."
    );
    AmsUnitModel::Ams2Pro
}

/// Builds and sends an `ams dry` cycle.
async fn run_dry(client: &mut Printer, args: DryArgs) -> Result<(), CliError> {
    let DryArgs {
        id,
        material,
        temp,
        duration_hours,
        rotate,
        filament,
        humidity,
        cooling_temp,
        close_power_conflict,
    } = args;

    let resolved = match material.as_deref() {
        None => None,
        Some(name) => match DryingMaterial::from_filament_type(name) {
            Some(m) => Some(m),
            None => {
                let known: Vec<&str> = DryingMaterial::all()
                    .iter()
                    .map(|m| m.wire_name())
                    .collect();
                return Err(CliError::from(Error::InvalidArgument(
                    format!(
                        "unknown material '{name}'. Known: {}. For anything else, pass --temp and --duration-hours with --filament '{name}'.",
                        known.join(", ")
                    )
                    .into(),
                )));
            }
        },
    };

    // Only pay the poll when a material actually needs a column chosen.
    let unit = match resolved {
        Some(_) => resolve_dry_unit(client, id).await,
        None => AmsUnitModel::Ams2Pro,
    };

    let mut cycle = client.dry(id).rotate_tray(rotate);
    if let Some(m) = resolved {
        cycle = cycle.material(m, unit);
    }
    // Explicit flags override the material; an unset flag keeps the material's value, or the
    // builder's default when no material was given.
    if let Some(t) = temp {
        cycle = cycle.temp(t);
    }
    if let Some(h) = duration_hours {
        cycle = cycle.duration_hours(h);
    }
    if let Some(f) = filament.as_deref() {
        cycle = cycle.filament(f);
    }
    if let Some(h) = humidity {
        cycle = cycle.humidity(h);
    }
    if let Some(c) = cooling_temp {
        cycle = cycle.cooling_temp(c);
    }
    if close_power_conflict {
        cycle = cycle.close_power_conflict(true);
    }

    let summary = match (resolved, temp, duration_hours) {
        (Some(m), _, _) => format!(
            "Starting AMS {id} drying cycle for {} ({unit:?} parameters)...",
            m.wire_name()
        ),
        (None, Some(t), Some(h)) => {
            format!("Starting AMS {id} drying cycle at {t}°C for {h} hours...")
        }
        _ => format!("Starting AMS {id} drying cycle..."),
    };

    dispatch(
        &summary,
        "AMS drying command published successfully.",
        cycle.send(),
    )
    .await?;
    Ok(())
}

/// Dispatches a typed control action to the printer.
pub async fn run(
    ip: &str,
    serial: &str,
    access_code: &str,
    action: ControlAction,
) -> Result<(), CliError> {
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
            let feedrate = feedrate.unwrap_or(3000);
            dispatch(
                "Dispatching motion G-code G0 relative move...",
                "Motion command published successfully.",
                client.move_relative(axis.as_char(), distance, feedrate),
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
                FanTargetArg::Left2 => FanTarget::AuxiliaryLeft2,
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
                io::stdin().read_line(&mut confirmation)?;
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
        ControlAction::Calibrate { routines, watch } => {
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
            // Under --watch stdout is the NDJSON capture stream, so status chatter goes to
            // stderr — a stray human-readable line would make the captured file invalid NDJSON.
            if watch {
                eprintln!("Triggering calibration routines...");
            } else {
                println!("Triggering calibration routines...");
            }
            // The client subscribes to the report topic during connect, before it publishes
            // anything (mqtt/client/mod.rs), so the subscription is already live by the time
            // this command lands — no push can be missed between trigger and follow loop.
            client.start_calibration(options).await?;
            if watch {
                eprintln!(
                    "Calibration command published. Following telemetry pushes as NDJSON — Ctrl+C to stop."
                );
                // `false` — capture every root, not just `print`. See issue #227: the
                // indicator being hunted may live under a root bambino does not model.
                return crate::monitor::follow_pushes(&mut client, false).await;
            }
            println!("Calibration command published successfully.");
        }
        ControlAction::Ams { action } => match action {
            AmsAction::Dry {
                id,
                material,
                temp,
                duration_hours,
                rotate,
                filament,
                humidity,
                cooling_temp,
                close_power_conflict,
            } => {
                run_dry(
                    &mut client,
                    DryArgs {
                        id,
                        material,
                        temp,
                        duration_hours,
                        rotate,
                        filament,
                        humidity,
                        cooling_temp,
                        close_power_conflict,
                    },
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
