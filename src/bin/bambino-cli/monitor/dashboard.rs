#![cfg(feature = "cli")]

use std::io::{self, Write};

use bambino::client::{FanTarget, PrintProgress, PrintSpeed, PrintStatus};
use bambino::types::SdcardState;

use crate::connection::Printer;

/// `write!`, ignoring the error — every `render_*` helper below targets an in-memory or
/// raw-mode terminal writer where a failed write means the terminal session is gone, which
/// the render loop has no useful way to react to (was `write!(...).unwrap_or(())`
/// duplicated at ~24 call sites).
macro_rules! dwrite {
    ($w:expr, $($arg:tt)*) => {
        ::std::write!($w, $($arg)*).unwrap_or(())
    };
}

/// `writeln!` sibling of [`dwrite!`].
macro_rules! dwriteln {
    ($w:expr, $($arg:tt)*) => {
        ::std::writeln!($w, $($arg)*).unwrap_or(())
    };
}

/// Write adapter that translates `\n` to `\r\n` for raw-mode terminal output.
pub(super) struct RawWriter<W: Write>(pub W);

impl<W: Write> Write for RawWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut last = 0;
        for (i, &byte) in buf.iter().enumerate() {
            if byte == b'\n' {
                if i > last {
                    self.0.write_all(&buf[last..i])?;
                }
                self.0.write_all(b"\r\n")?;
                last = i + 1;
            }
        }
        if last < buf.len() {
            self.0.write_all(&buf[last..])?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// Redraws the dashboard from the client's cached telemetry.
///
/// Everything shown comes from `PrinterClient`'s typed accessors after `poll_telemetry()` has
/// folded the latest frame in, so the dashboard shows exactly what a library consumer sees and
/// inherits every decode fix (#600) instead of re-reading the raw JSON by wire name.
///
/// `warning` is the monitor loop's most recent non-fatal diagnostic, rendered in the footer
/// through the same [`RawWriter`] as everything else. It cannot go to `log::warn!`: the CLI's
/// logger writes to the tty this dashboard has put in raw mode, so a record would land
/// mid-screen at the current cursor with stair-stepped line breaks.
pub(super) fn draw_dashboard(printer: &Printer, warning: Option<&str>) {
    let mut w = RawWriter(io::stdout());
    dwrite!(w, "\x1B[1;1H\x1B[2J");

    render_print_status(
        printer.print_status(),
        printer.subtask_name(),
        printer.print_progress(),
        (printer.print_speed(), printer.print_speed_magnitude()),
        &mut w,
    );
    render_nozzles(printer, &mut w);
    render_thermal(printer, &mut w);
    render_fans_and_system(printer, &mut w);
    render_ams(printer, &mut w);
    render_external_spool(printer, &mut w);

    dwriteln!(
        w,
        "======================================================================="
    );

    render_diagnostics(printer, &mut w);

    if let Some(warning) = warning {
        dwriteln!(w, "\n\x1B[33m! {}\x1B[0m", warning);
    }

    dwriteln!(w, "\n\x1B[2m[q/x/Esc to quit]\x1B[0m");
    w.flush().unwrap_or(());
}

/// `--` for an unobserved value.
fn or_dash(value: Option<String>) -> String {
    value.unwrap_or_else(|| "--".to_string())
}

fn render_print_status(
    status: Option<PrintStatus>,
    subtask_name: Option<&str>,
    progress: PrintProgress,
    (speed, magnitude): (Option<PrintSpeed>, Option<u16>),
    w: &mut impl Write,
) {
    let percent = progress.percent.unwrap_or(0);
    let layer_num = progress.layer_num.unwrap_or(0);
    let total_layers = progress.total_layers.unwrap_or(0);
    let remaining_sec = i64::from(progress.remaining_secs.unwrap_or(0));
    let remaining_formatted = if remaining_sec > 0 {
        format!("{}m {}s", remaining_sec / 60, remaining_sec % 60)
    } else {
        String::from("--")
    };

    dwriteln!(
        w,
        "================== Bambu Lab Printer Live Dashboard ==================="
    );
    dwriteln!(
        w,
        "{:<20} : {}",
        "Operational State",
        or_dash(status.map(|s| format!("{s:?}")))
    );
    dwriteln!(
        w,
        "{:<20} : {}",
        "Active Job Name",
        subtask_name.unwrap_or("None")
    );
    dwriteln!(
        w,
        "{:<20} : {}%  ({}/{})",
        "Print Progress",
        percent,
        layer_num,
        total_layers
    );
    dwriteln!(w, "{:<20} : {}", "Time Remaining", remaining_formatted);
    dwriteln!(
        w,
        "{:<20} : {} ({})",
        "Print Speed",
        or_dash(speed.map(|s| format!("{s:?}"))),
        or_dash(magnitude.map(|m| format!("{m}%")))
    );
}

fn render_nozzles(printer: &Printer, w: &mut impl Write) {
    // Installed nozzles from `device.nozzle.info` (rack-stored spares excluded), else one
    // nozzle 0; temperatures from the library's cross-model decode.
    let mut rows: Vec<(u8, String)> = printer
        .device()
        .and_then(|d| d.nozzle.as_ref())
        .and_then(|n| n.info.as_deref())
        .unwrap_or(&[])
        .iter()
        .filter(|n| !n.is_rack_stored())
        .map(|n| {
            let diameter = n
                .diameter
                .map_or_else(|| "--".to_string(), |d| format!("{d:.1}mm"));
            let ntype = n.nozzle_type.as_deref().unwrap_or("--");
            (n.id, format!("{diameter} {ntype}"))
        })
        .collect();
    if rows.is_empty() {
        let (diameter, ntype) = printer.legacy_nozzle();
        rows.push((
            0,
            format!("{}mm {}", diameter.unwrap_or("--"), ntype.unwrap_or("--")),
        ));
    }
    let temps = printer.nozzle_temperatures();

    dwriteln!(
        w,
        "\n--- Nozzles -----------------------------------------------------------"
    );
    let cells: Vec<String> = rows
        .iter()
        .map(|(id, label)| match temps.iter().find(|t| t.id == *id) {
            Some(t) => format!("#{id}: {label} ({}°C / T: {}°C)", t.actual, t.target),
            None => format!("#{id}: {label}"),
        })
        .collect();
    for pair in cells.chunks(2) {
        match pair {
            [a, b] => dwriteln!(w, "{:<34} │ {}", a, b),
            [a] => dwriteln!(w, "{}", a),
            _ => {}
        }
    }
}

fn render_thermal(printer: &Printer, w: &mut impl Write) {
    dwriteln!(
        w,
        "\n--- Thermal -----------------------------------------------------------"
    );
    let heater = |t: Option<bambino::client::HeaterTemps>| match t {
        Some(t) => format!("{:>3}°C / {:>3}°C", t.actual, t.target),
        None => "--".to_string(),
    };
    dwriteln!(
        w,
        "{:<20} : {}",
        "Heated Bed",
        heater(printer.bed_temperatures())
    );
    if printer.quirks().has_chamber_temperature_sensor() {
        dwriteln!(
            w,
            "{:<20} : {}",
            "Chamber",
            heater(printer.chamber_temperature())
        );
    }
}

fn render_fans_and_system(printer: &Printer, w: &mut impl Write) {
    let pct = |v: Option<u8>| or_dash(v.map(|p| format!("{p}%")));
    let quirks = printer.quirks();
    let mut fans: Vec<(&str, String)> = vec![
        (
            "Part Cooling",
            pct(printer.fan_speed(FanTarget::PartCooling)),
        ),
        ("Aux Fan", pct(printer.fan_speed(FanTarget::AuxiliaryLeft))),
        (
            "Chamber Fan",
            pct(printer.fan_speed(FanTarget::ChamberExhaust)),
        ),
        ("Heatbreak Fan", pct(printer.heatbreak_fan_speed())),
    ];
    if FanTarget::AuxiliaryLeft2.is_supported_by(quirks) {
        fans.push((
            "Aux Fan 2",
            pct(printer.fan_speed(FanTarget::AuxiliaryLeft2)),
        ));
    }

    let sdcard = match printer.sdcard_status() {
        Some(SdcardState::Normal) => "Inserted",
        Some(SdcardState::NoSdcard) => "Not Detected",
        Some(SdcardState::Abnormal) => "Abnormal",
        Some(SdcardState::ReadOnly) => "Read-only",
        None => "--",
    };
    let toggle = |v: Option<bool>| match v {
        Some(true) => "enable",
        Some(false) => "disable",
        None => "--",
    };
    let ipcam = printer.ipcam();
    let system: [(&str, String); 4] = [
        ("WiFi", printer.wifi_signal().unwrap_or("--").to_string()),
        ("SD Card", sdcard.to_string()),
        (
            "Recording",
            toggle(ipcam.and_then(|i| i.recording())).to_string(),
        ),
        (
            "Timelapse",
            toggle(ipcam.and_then(|i| i.timelapse_enabled())).to_string(),
        ),
    ];

    dwriteln!(
        w,
        "\n--- Fans & System -----------------------------------------------------"
    );
    for i in 0..fans.len().max(system.len()) {
        let (fan_label, fan_value) = fans.get(i).map_or(("", ""), |(l, v)| (*l, v.as_str()));
        let (sys_label, sys_value) = system.get(i).map_or(("", ""), |(l, v)| (*l, v.as_str()));
        dwriteln!(
            w,
            "{:<14} : {:<6} {:>3} {:<14} : {}",
            fan_label,
            fan_value,
            "│",
            sys_label,
            sys_value
        );
    }
}

fn render_ams(printer: &Printer, w: &mut impl Write) {
    // `sanitized_ams()` applies the library's stale-tray rules, including AMS-HT's.
    let Some(ams) = printer.sanitized_ams() else {
        return;
    };
    for unit in &ams.ams {
        let temp = unit.temp.as_deref().unwrap_or("--");
        let humidity = unit
            .humidity_raw
            .as_deref()
            .and_then(|h| h.trim().parse::<u64>().ok())
            .map(|h| format!("{h}%"))
            .or_else(|| unit.humidity.clone())
            .unwrap_or_else(|| "--".to_string());
        let dry_suffix = match unit.dry_time {
            Some(mins) if mins > 0 => {
                let dry_temp = unit
                    .dry_setting
                    .as_ref()
                    .and_then(|ds| ds.dry_temperature)
                    .filter(|t| *t > 0);
                match dry_temp {
                    Some(t) => format!(" Drying: {}:{:02}@{}°C", mins / 60, mins % 60, t),
                    None => format!(" Drying: {}:{:02} left", mins / 60, mins % 60),
                }
            }
            _ => String::new(),
        };

        let header = format!(
            "\n--- AMS #{} ({}°C, RH:{}){}",
            unit.id, temp, humidity, dry_suffix
        );
        let pad = 71usize.saturating_sub(header.chars().count() - 1);
        dwriteln!(w, "{} {}", header, "-".repeat(pad));

        let Some(trays) = unit.tray.as_deref() else {
            continue;
        };
        let ams_id = unit.ams_id();
        let mut table = crate::table::Table::new(vec!["Slot", "Status", "Material", "Remaining"]);
        for tray in trays {
            let material = tray.material().unwrap_or("");
            let status = match tray.state {
                Some(11) => "Loaded",
                _ if ams_id.is_some_and(|id| tray.is_loaded(id)) => "Present",
                _ => "Empty",
            };
            let remain = tray
                .remain_percent()
                .map(|r| format!("{r}%"))
                .unwrap_or_default();
            table.add_row(vec![&tray.id, status, material, &remain]);
        }
        table.write_to(w);
    }
}

fn render_external_spool(printer: &Printer, w: &mut impl Write) {
    let Some(vt) = printer.vt_tray() else {
        return;
    };
    let Some(material) = vt.material() else {
        return;
    };
    let color_swatch = format_color_swatch(vt.tray_color.as_deref().unwrap_or(""));
    dwriteln!(
        w,
        "\n--- External Spool ----------------------------------------------------"
    );
    dwriteln!(
        w,
        "{:<20} : {} {} (max {}°C)",
        "Material",
        material,
        color_swatch,
        vt.nozzle_temp_max.as_deref().unwrap_or("--")
    );
}

fn render_diagnostics(printer: &Printer, w: &mut impl Write) {
    if let Some(fault) = printer.active_fault()
        && fault.is_genuine_fault
    {
        dwriteln!(
            w,
            "\x1B[1;31m[ACTIVE ERROR] Code: {}\x1B[0m",
            fault.short_code
        );
    }

    let alerts = printer.active_hms_alerts();
    if !alerts.is_empty() {
        dwriteln!(w, "Active Hardware Alerts:");
        for decoded in &alerts {
            dwriteln!(
                w,
                "  \x1B[1;33m[{}] Severity: {:?} (Module: {})\x1B[0m",
                decoded.short_code,
                decoded.severity,
                decoded.module_id
            );
        }
    }
}

/// Renders a terminal-escape color swatch from a `tray_color` hex string off the wire.
///
/// `hex_color` is untrusted printer telemetry. `.len() < 6` alone only checks *byte*
/// length, not char-boundary safety — a value with 6+ bytes but containing a multi-byte
/// UTF-8 character positioned so byte offset 2, 4, or 6 falls mid-codepoint would panic
/// ("byte index N is not a char boundary") on the raw `&hex_color[0..2]`-style slices
/// below. Checking that the first 6 bytes are all ASCII first guarantees each of those
/// bytes is its own character, so slicing at any point within that prefix is always a
/// valid char boundary.
fn format_color_swatch(hex_color: &str) -> String {
    if hex_color.len() < 6 || !hex_color.as_bytes()[..6].is_ascii() {
        return String::new();
    }
    let r = u8::from_str_radix(&hex_color[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex_color[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex_color[4..6], 16).unwrap_or(0);
    format!("\x1B[48;2;{};{};{}m  \x1B[0m", r, g, b)
}

#[cfg(test)]
mod format_color_swatch_tests {
    use super::format_color_swatch;

    #[test]
    fn test_format_color_swatch_valid_hex() {
        let swatch = format_color_swatch("FF00FF");
        assert!(swatch.contains("255;0;255"));
    }

    #[test]
    fn test_format_color_swatch_too_short_returns_empty() {
        assert_eq!(format_color_swatch("FF00"), "");
    }

    #[test]
    fn test_format_color_swatch_multibyte_char_does_not_panic() {
        // Regression test: a value with >= 6 bytes but a multi-byte UTF-8 character
        // straddling a byte offset the old code sliced at (0, 2, 4, or 6) used to panic
        // with "byte index N is not a char boundary" instead of degrading cleanly.
        // "a" (1 byte) + "é" (2 bytes, occupying byte offsets 1-2) + "2345" (4 bytes) = 7
        // bytes total; the old `&hex_color[0..2]` slice's end boundary (byte offset 2)
        // fell in the middle of 'é'.
        assert_eq!(format_color_swatch("a\u{e9}2345"), "");
    }
}

#[cfg(test)]
mod print_status_tests {
    use super::render_print_status;
    use bambino::client::{PrintProgress, PrintSpeed, PrintStatus};

    fn render(progress: PrintProgress) -> String {
        let mut out: Vec<u8> = Vec::new();
        render_print_status(
            Some(PrintStatus::Running),
            Some("JEFF+DOG"),
            progress,
            (Some(PrintSpeed::Standard), Some(100)),
            &mut out,
        );
        String::from_utf8(out).expect("utf8")
    }

    #[test]
    fn test_print_progress_renders_the_client_cache() {
        let rendered = render(PrintProgress {
            percent: Some(68),
            // 99 wire minutes -> seconds, as `update_progress_cache` converts it.
            remaining_secs: Some(5940),
            layer_num: Some(516),
            total_layers: Some(879),
        });
        assert!(rendered.contains("68%  (516/879)"), "{rendered}");
        assert!(rendered.contains("99m 0s"), "{rendered}");
        assert!(rendered.contains("Running"), "{rendered}");
        assert!(rendered.contains("Standard (100%)"), "{rendered}");
    }
}
