#![cfg(feature = "cli")]

//! # Network Discovery Subcommand Handler
//!
//! Executes standard SSDP active searches on Port 2021 utilizing the `bambino`
//! asynchronous discovery engine [REF-NET-DISC]. Prints details of detected printers
//! to standard output.

use std::time::Duration;

use bambino::discovery::discover_devices;
use bambino::error::BambuError;
use bambino::io::tokio::{TokioTimer, TokioUdpSocket};

/// Initiates an active multicast SSDP search sweep and displays nearby printers.
pub async fn run() -> Result<(), BambuError> {
    let is_verbose = crate::is_verbose();
    println!("Scanning for printers (20 seconds)...");
    log::debug!("Resolving network discovery sweep targets utilizing standard Tokio UDP socket");

    // Port behavior varies by model: the P1S (firmware 01.10.00.00) responds to M-SEARCH
    // on port 1990 within ~5s but only sends passive NOTIFY on port 2021 at ~10.1s intervals.
    // A 20-second window covers both discovery paths across model generations.
    let devices =
        discover_devices::<TokioUdpSocket, TokioTimer>(Duration::from_secs(20), &TokioTimer::new())
            .await?;

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
    if is_verbose {
        let mut table = crate::table::Table::new(vec![
            "Model",
            "Serial",
            "IP Address",
            "Name",
            "Firmware",
            "SSDP Port",
        ]);
        for device in &devices {
            table.add_row(vec![
                &format!("{:?}", device.model),
                &device.serial,
                &device.ip,
                &device.name,
                &device.version,
                &device.discovery_port.to_string(),
            ]);
        }
        table.print();
    } else {
        let mut table =
            crate::table::Table::new(vec!["Model", "Serial", "IP Address", "Name", "Firmware"]);
        for device in &devices {
            table.add_row(vec![
                &format!("{:?}", device.model),
                &device.serial,
                &device.ip,
                &device.name,
                &device.version,
            ]);
        }
        table.print();
    }

    Ok(())
}
