//! # Implicit FTPS State Client & Storage Traversal
//!
//! Exposes secure file transfer commands, passive socket connection builders,
//! and UNIX-style directory listing parsers to interface with the printer's
//! local MicroSD card.
//!
//! Consolidates implementation structures under a unified module namespace,
//! simplifying local storage interaction across standard, RTOS, and bare-metal environments.

pub mod client;
pub mod parser;
pub(crate) mod protocol;

pub use client::{BambuFtpsClient, FtpDataStreamFactory};
pub use parser::{FtpFile, parse_unix_listing};
