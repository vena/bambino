//! # Port-6000 File Tunnel
//!
//! Client for the TLS file service newer printers run on port 6000 [REF-FILESVC]
//! (`reference/08_file_service.md`). It reaches internal (eMMC) storage, which FTPS on port 990
//! does not, and reads members of the running job's `.3mf` without a path.
//!
//! Which printers serve it: those whose telemetry sets `fun` bit 31, which BambuStudio reads as
//! `is_support_brtc` ("support tcp and upload protocol") and uses to choose this service over
//! FTPS for uploads. P1 and A1 firmware sends no `fun` and serves only the chamber-image stream
//! on this port; a P1S ignores this tunnel's login and closes the connection after 60 s.
//!
//! [`FileTunnelClient`] speaks the protocol over an already-dialed raw stream, like
//! [`FtpsClient`](crate::ftps::FtpsClient). The two services address files differently (a card
//! path against a storage label plus the `path` a listing returns), so neither wraps the other.

pub mod client;
pub(crate) mod protocol;

pub use client::{FileTunnel, FileTunnelClient};

/// Port the file tunnel listens on, shared with the chamber-image stream of P1/A1 printers.
pub const FILE_TUNNEL_PORT: u16 = 6000;

/// Default largest frame payload [`FileTunnelClient`] accepts, in bytes (4 MiB).
///
/// A corrupt or hostile header could otherwise make the reader allocate up to 4 GiB, which on a
/// `no_std` target aborts through `alloc_error_handler` rather than returning an error. 4 MiB
/// leaves room for a long `LIST_INFO` reply on a host, but is far more than an embedded heap
/// holds: lower it there with [`FileTunnelClient::with_max_frame_payload`] or
/// `PrinterClient::with_file_tunnel_max_frame_size`.
pub const DEFAULT_MAX_FRAME_PAYLOAD: usize = 4 * 1024 * 1024;

#[cfg(not(feature = "std"))]
use alloc::string::String;

/// Which kind of file a listing asks for: `LIST_INFO`'s `type`, and the kind `PrinterClient::list_files` lists over either service.
///
/// Re-exported as `bambino::client::FileKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileKind {
    /// Timelapse videos.
    Timelapse,
    /// Recorded videos.
    Video,
    /// Print files (`.3mf`).
    Model,
}

impl FileKind {
    /// The wire value Studio sends (`PrinterFileSystem::ListAllFiles`).
    pub(crate) fn as_wire(self) -> &'static str {
        match self {
            FileKind::Timelapse => "timelapse",
            FileKind::Video => "video",
            FileKind::Model => "model",
        }
    }
}

/// One file from a [`FileTunnelClient::list_files`] listing.
///
/// Pass it back to [`FileTunnelClient::download`] unchanged: it addresses the file by `path` when
/// the printer gave one and by `name` otherwise, as Studio does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelFile {
    /// The file name.
    pub name: String,
    /// The printer's path for the file, or `None` when the listing gave none.
    pub path: Option<String>,
    /// Modification time, Unix seconds; `0` when the listing gave none.
    pub time: u64,
    /// Size in bytes.
    pub size: u64,
    /// The storage label the listing was made against, or `None` for the printer's default.
    pub storage: Option<String>,
}

/// Names a file tunnel `result` code as BambuStudio's `PrinterFileSystem.h` does, or `None` for a code it doesn't define.
///
/// Codes 3–6 are also Studio's own client-side failures, so seeing one from a printer is unusual.
pub fn result_name(result: i64) -> Option<&'static str> {
    Some(match result {
        0 => "SUCCESS",
        1 => "CONTINUE",
        2 => "ERROR_JSON",
        3 => "ERROR_PIPE",
        4 => "ERROR_CANCEL",
        5 => "ERROR_RES_BUSY",
        6 => "ERROR_TIME_OUT",
        10 => "FILE_NO_EXIST",
        11 => "FILE_NAME_INVALID",
        12 => "FILE_SIZE_ERR",
        13 => "FILE_OPEN_ERR",
        14 => "FILE_READ_WRITE_ERR",
        15 => "FILE_CHECK_ERR",
        16 => "FILE_TYPE_ERR",
        17 => "STORAGE_UNAVAILABLE",
        18 => "API_VERSION_UNSUPPORT",
        19 => "FILE_EXIST",
        20 => "STORAGE_SPACE_NOT_ENOUGH",
        21 => "FILE_CREATE_ERR",
        22 => "FILE_WRITE_ERR",
        23 => "MD5_COMPARE_ERR",
        24 => "FILE_RENAME_ERR",
        25 => "SEND_ERR",
        _ => return None,
    })
}
