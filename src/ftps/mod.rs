//! # FTPS File Transfer Client
//!
//! Implicit FTPS client for reading and writing files on the printer's SD card.
//!
//! [`BambuFtpsClient`] handles the TLS control channel, passive-mode data connections,
//! and FTP command sequencing. It supports listing directories, uploading/downloading
//! files, checking free space, and basic file management (rename, delete, mkdir).
//! The [`parser`] submodule handles UNIX-style directory listing output.

pub mod client;
pub mod parser;
pub(crate) mod protocol;

pub use client::{BambuFtpsClient, FtpDataStreamFactory};
pub use parser::{FtpFile, parse_unix_listing};
