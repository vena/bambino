#![cfg(feature = "std")]

//! # Network Discovery Subcommand Handler
//!
//! Executes standard SSDP active searches on Port 2021 utilizing the `bambu-lan`
//! asynchronous discovery engine [REF-NET-DISC]. Prints details of detected printers
//! to standard output.

use std::time::Duration;

use bambu_lan::discovery::discover_devices;
use bambu_lan::error::BambuError;
use bambu_lan::io::tokio::{TokioTimer, TokioUdpSocket};

/// Initiates an active multicast SSDP search sweep and displays nearby printers.
pub async fn run() -> Result<(), BambuError> {
    println!("Initiating local network discovery sweep (duration: 3 seconds)...");

    // Perform the asynchronous scan using our tokio and timer platform bindings
    let devices =
        discover_devices::<TokioUdpSocket, TokioTimer>(Duration::from_secs(3), &TokioTimer).await?;

    if devices.is_empty() {
        println!("No Bambu Lab printers detected. Ensure LAN Mode is enabled on the printer.");
        return Ok(());
    }

    println!("\nDetected {} Printer(s):", devices.len());
    println!("{:=<100}", "");
    println!(
        "{:<18} | {:<16} | {:<15} | {:<25} | {:<15}",
        "Model", "Serial Number", "IP Address", "Device Name", "Firmware Version"
    );
    println!("{:=<100}", "");

    for device in devices {
        let model_str = format!("{:?}", device.model);
        println!(
            "{:<18} | {:<16} | {:<15} | {:<25} | {:<15}",
            model_str, device.serial, device.ip, device.name, device.version
        );
    }
    println!("{:=<100}\n", "");

    Ok(())
}
