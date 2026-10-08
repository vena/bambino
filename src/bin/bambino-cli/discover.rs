#![cfg(feature = "cli")]

//! # Network Discovery Subcommand Handler
//!
//! Executes standard SSDP active searches on Port 2021 utilizing the `bambino`
//! asynchronous discovery engine [REF-NET-DISC]. Prints details of detected printers
//! to standard output.

use std::time::Duration;

use bambino::discovery::discover;

use crate::error::CliError;

/// Discovery sweep length. The P1S (firmware 01.10.00.00) answers M-SEARCH on neither port and
/// is found only through NOTIFY advertisements, ~10.1s apart on port 2021 with some in between
/// on 1990. 20 seconds covers a full 2021 cycle with margin.
const DISCOVERY_WINDOW_SECS: u64 = 20;

/// Initiates an active multicast SSDP search sweep and displays nearby printers.
pub async fn run() -> Result<(), CliError> {
    let is_verbose = crate::is_verbose();
    println!("Scanning for printers ({DISCOVERY_WINDOW_SECS} seconds)...");
    log::debug!("Resolving network discovery sweep targets utilizing standard Tokio UDP socket");

    let devices = discover(Duration::from_secs(DISCOVERY_WINDOW_SECS)).await?;

    if devices.is_empty() {
        println!("\nNo Bambu Lab printers detected. Ensure LAN Mode is active on the printer.");
        if is_verbose {
            println!("\nDiagnostic hints — why did discovery return zero devices?");
            println!(
                "  1. Firewall Restrictions: Ensure inbound/outbound UDP traffic on local Port 2021 is permitted."
            );
            println!(
                "  2. IGMP Snooping: Some modern routers drop multicast packets (239.255.255.250) sent over Wi-Fi."
            );
            println!(
                "  3. VPN/Virtual Adapters: If you are running active virtual adapters (Docker, WSL, VirtualBox),"
            );
            println!(
                "     the OS may route UDP broadcast queries over the wrong adapter interface."
            );
            println!(
                "     Try running the command with your VPN or virtual interfaces disabled.\n"
            );
        }
        return Ok(());
    }

    println!("\nDetected {} printer(s):\n", devices.len());
    let mut headers = vec!["Model", "Serial", "IP Address", "Name", "Firmware"];
    if is_verbose {
        // The port this printer was first heard on, not the printer's own port: one that
        // advertises on both reports whichever arrived first. The -v log lists every packet.
        headers.push("Heard On");
    }
    let mut table = crate::table::Table::new(headers);
    for device in &devices {
        let model = device.model.to_string();
        let ip = device.ip.to_string();
        let ssdp_port = device
            .discovery_port
            .map_or_else(|| "?".to_owned(), |p| p.to_string());
        let mut row = vec![
            model.as_str(),
            &device.serial,
            &ip,
            &device.name,
            &device.version,
        ];
        if is_verbose {
            row.push(&ssdp_port);
        }
        table.add_row(row);
    }
    table.print();

    Ok(())
}
