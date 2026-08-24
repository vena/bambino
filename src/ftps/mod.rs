//! # FTPS File Transfer Client
//!
//! Implicit FTPS client for reading and writing files on the printer's SD card.
//!
//! [`FtpsClient`] handles the TLS control channel, passive-mode data connections,
//! and FTP command sequencing. It supports listing directories, uploading/downloading
//! files, checking free space, and basic file management (rename, delete, mkdir).
//! The [`parser`] submodule handles UNIX-style directory listing output.

pub mod client;
pub mod parser;
pub(crate) mod protocol;

pub(crate) const FTPS_PORT: u16 = 990;

pub use client::FtpsClient;
pub use parser::{
    CurrentDateTime, FtpFile, FtpTimestamp, parse_mdtm_timestamp, parse_unix_listing,
};
