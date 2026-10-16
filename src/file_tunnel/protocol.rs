//! Wire format of the port-6000 file tunnel [REF-FILESVC] (`reference/08_file_service.md` §8.2–8.4).
//!
//! Two layers with different provenance. The JSON request and reply bodies are BambuStudio's
//! (`PrinterFileSystem`), which is authoritative for them. The framing (the 16-byte header, the
//! login and setup frames, the `mtype` the plugin adds to each request) lives in Bambu's closed
//! network plugin, so Studio's source cannot show it. It is taken from ha-bambulab's
//! `Tcp6000MediaSource` (`pybambu/media_sources.py`) and open-bamboo-networking's
//! `src/tunnel_local.cpp`: two independent working clients that agree byte for byte.

#[cfg(not(feature = "std"))]
use alloc::{format, string::String, vec::Vec};

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::identity::{ACCESS_CODE_LEN, LAN_USERNAME, validate_access_code};

/// Length of the header in front of every frame, in both directions.
pub(crate) const FRAME_HEADER_LEN: usize = 16;

/// Magic of the client's login frame (subchannel `01`, client to printer).
pub(crate) const MAGIC_LOGIN: u32 = 0x0101_013F;
/// Magic of the printer's login acknowledgement (subchannel `01`, printer to client).
pub(crate) const MAGIC_LOGIN_ACK: u32 = 0x0001_013F;
/// Magic of every client control frame: setup and each request (subchannel `02`).
pub(crate) const MAGIC_CTRL: u32 = 0x0102_013F;
/// Magic of the printer's control frames: the setup reply, replies and streamed data.
pub(crate) const MAGIC_CTRL_REPLY: u32 = 0x0002_013F;
/// Magic of the frame a printer sends before closing a session it refuses (open-bamboo-networking only).
pub(crate) const MAGIC_REJECT: u32 = 0x0003_013F;

/// Width of each NUL-padded field of the login payload: the username, then the access code.
pub(crate) const LOGIN_FIELD_LEN: usize = 8;
/// Length of the login frame's payload.
pub(crate) const LOGIN_PAYLOAD_LEN: usize = 2 * LOGIN_FIELD_LEN;
const _: () = assert!(ACCESS_CODE_LEN <= LOGIN_FIELD_LEN);
const _: () = assert!(LAN_USERNAME.len() <= LOGIN_FIELD_LEN);

/// `mtype` of a control request, and the `req.mtype` the setup frame asks for (Studio's `CTRL_TYPE`, `0x3001`).
pub(crate) const MTYPE_CTRL: u32 = 0x3001;
/// `mtype` of the setup frame and its reply (`0x3003`).
pub(crate) const MTYPE_SETUP: u32 = 0x3003;
/// The setup frame's `req.t_av`: `1` from Studio's file browser (`libBambuSource`).
pub(crate) const SETUP_T_AV: u8 = 1;
/// The setup frame's `req.peer_t`.
pub(crate) const SETUP_PEER_T: u8 = 3;
/// The setup frame's `req.ver`, the client version both reference clients send.
pub(crate) const SETUP_VER: &str = "02.03.00.00";

/// `req.peer` Studio sends with `REQUEST_MEDIA_ABILITY`.
pub(crate) const PEER_STUDIO: &str = "studio";
/// `req.api_version` Studio sends with `LIST_INFO` and `REQUEST_MEDIA_ABILITY`.
pub(crate) const API_VERSION: u32 = 2;
/// `req.notify` Studio sends with `LIST_INFO`.
pub(crate) const LIST_NOTIFY_DETAIL: &str = "DETAIL";

/// Separates a frame's JSON from binary that follows it in the same payload, in both directions.
pub(crate) const BINARY_SEPARATOR: &[u8] = b"\n\n";

/// `cmdtype` values, from Studio's `PrinterFileSystem.h`.
pub(crate) mod cmd {
    /// List files of one type on one storage.
    pub(crate) const LIST_INFO: u32 = 0x0001;
    /// Read members of a listed file without downloading it.
    pub(crate) const SUB_FILE: u32 = 0x0002;
    /// Delete files.
    pub(crate) const FILE_DEL: u32 = 0x0003;
    /// Download a file, or an in-memory file under `mem:/<index>`.
    pub(crate) const FILE_DOWNLOAD: u32 = 0x0004;
    /// Chunked upload.
    pub(crate) const FILE_UPLOAD: u32 = 0x0005;
    /// List the storages this printer has.
    pub(crate) const REQUEST_MEDIA_ABILITY: u32 = 0x0007;
    /// First printer-initiated notify `cmdtype`; a frame with no `result` and a `cmdtype` at or above it is a notify.
    pub(crate) const NOTIFY_FIRST: u32 = 0x0100;
    /// Cancel in-flight requests by `sequence`.
    pub(crate) const TASK_CANCEL: u32 = 0x1000;
}

/// `result` values the client acts on, from Studio's `PrinterFileSystem.h`; the rest only reach an error message.
pub(crate) mod result {
    /// The request finished.
    pub(crate) const SUCCESS: i64 = 0;
    /// More frames follow under the same `sequence`.
    pub(crate) const CONTINUE: i64 = 1;
    /// Malformed JSON; also Studio's code for a `mem:/` status blob that refuses the read.
    pub(crate) const ERROR_JSON: i64 = 2;
    /// Wrong size; also Studio's code for a `mem:/` status blob reporting size 0.
    pub(crate) const FILE_SIZE_ERR: i64 = 12;
    /// The upload's target name exists; Studio continues, and the upload overwrites it.
    pub(crate) const FILE_EXIST: i64 = 19;
}

/// One decoded frame header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameHeader {
    /// Length of the payload after the header.
    pub(crate) payload_len: u32,
    /// One of the `MAGIC_*` values.
    pub(crate) magic: u32,
    /// The sender's per-frame counter; not the JSON `sequence`.
    pub(crate) seq: u32,
}

impl FrameHeader {
    /// Encodes the header: `payload_len | magic | seq | 4 zero bytes`, all little-endian.
    pub(crate) fn encode(&self) -> [u8; FRAME_HEADER_LEN] {
        let mut out = [0u8; FRAME_HEADER_LEN];
        out[0..4].copy_from_slice(&self.payload_len.to_le_bytes());
        out[4..8].copy_from_slice(&self.magic.to_le_bytes());
        out[8..12].copy_from_slice(&self.seq.to_le_bytes());
        out
    }

    /// Decodes a header, ignoring the four reserved bytes, which neither reference client checks.
    pub(crate) fn decode(bytes: &[u8; FRAME_HEADER_LEN]) -> Self {
        let word = |at: usize| {
            u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        Self {
            payload_len: word(0),
            magic: word(4),
            seq: word(8),
        }
    }
}

/// Builds the login payload: `bblp`, then the access code, each NUL-padded to 8 bytes.
///
/// This is not the chamber-image stream's 80-byte auth [REF-CAM-BINARY]; a printer serving that
/// stream ignores this login (a P1S on `01.10.00.00` holds the connection silently for 60 s).
pub(crate) fn build_login_payload(access_code: &str) -> Result<[u8; LOGIN_PAYLOAD_LEN], Error> {
    validate_access_code(access_code)?;
    let mut out = [0u8; LOGIN_PAYLOAD_LEN];
    let user = LAN_USERNAME.as_bytes();
    out[..user.len()].copy_from_slice(user);
    let code = access_code.as_bytes();
    out[LOGIN_FIELD_LEN..LOGIN_FIELD_LEN + code.len()].copy_from_slice(code);
    Ok(out)
}

#[derive(Serialize)]
struct SetupReq<'a> {
    t_av: u8,
    mtype: u32,
    peer_t: u8,
    pid: &'a str,
    ver: &'a str,
}

#[derive(Serialize)]
struct Setup<'a> {
    sequence: u32,
    mtype: u32,
    req: SetupReq<'a>,
}

/// Builds the setup frame's JSON, sent once after the login is acknowledged.
///
/// `pid` is an 8-hex-digit client id; both reference clients derive it from their frame counter.
pub(crate) fn build_setup_json(pid: &str) -> Result<Vec<u8>, Error> {
    crate::client::serialize(&Setup {
        sequence: 0,
        mtype: MTYPE_SETUP,
        req: SetupReq {
            t_av: SETUP_T_AV,
            mtype: MTYPE_CTRL,
            peer_t: SETUP_PEER_T,
            pid,
            ver: SETUP_VER,
        },
    })
}

/// Formats a client id for the setup frame from a frame counter value.
pub(crate) fn setup_pid(seq: u32) -> String {
    format!("{seq:08x}")
}

#[derive(Serialize)]
struct Request<'a, R: Serialize> {
    mtype: u32,
    cmdtype: u32,
    sequence: u32,
    req: &'a R,
}

/// Encodes one request payload: `{"mtype":12289,"cmdtype":..,"sequence":..,"req":{..}}`, then `\n\n` and `binary` when it is non-empty.
///
/// Studio builds the JSON without `mtype`; the plugin prepends it on the wire, and both reference
/// clients send it first, as here.
pub(crate) fn encode_request<R: Serialize>(
    cmdtype: u32,
    sequence: u32,
    req: &R,
    binary: &[u8],
) -> Result<Vec<u8>, Error> {
    let mut out = crate::client::serialize(&Request {
        mtype: MTYPE_CTRL,
        cmdtype,
        sequence,
        req,
    })?;
    if !binary.is_empty() {
        out.reserve(BINARY_SEPARATOR.len() + binary.len());
        out.extend_from_slice(BINARY_SEPARATOR);
        out.extend_from_slice(binary);
    }
    Ok(out)
}

/// Splits a payload into its JSON and the binary after it.
///
/// The JSON runs to the first `\n\n`, as in Studio's `PrinterFileSystem::HandleResponse`; with no
/// separator the whole payload is JSON. The printer sends compact JSON, whose strings escape any
/// newline, so the first `\n\n` cannot fall inside it.
pub(crate) fn split_payload(payload: &[u8]) -> (&[u8], &[u8]) {
    match payload
        .windows(BINARY_SEPARATOR.len())
        .position(|w| w == BINARY_SEPARATOR)
    {
        Some(at) => (&payload[..at], &payload[at + BINARY_SEPARATOR.len()..]),
        None => (payload, &[]),
    }
}

/// A reply or notify envelope, with its body typed by the command.
///
/// Studio tells the two apart by `result`: a reply carries one, a notify does not and puts its
/// body under `notify` instead. Unknown keys are ignored.
#[derive(Debug, Deserialize)]
pub(crate) struct Envelope<T> {
    #[serde(default)]
    pub(crate) cmdtype: Option<u32>,
    #[serde(default)]
    pub(crate) sequence: Option<u32>,
    #[serde(default)]
    pub(crate) result: Option<i64>,
    #[serde(default = "Option::default")]
    pub(crate) reply: Option<T>,
}

impl<T> Envelope<T> {
    /// Whether this frame is a printer-initiated notify rather than a reply.
    pub(crate) fn is_notify(&self) -> bool {
        self.result.is_none() && self.cmdtype.is_some_and(|c| c >= cmd::NOTIFY_FIRST)
    }
}

/// Parses a frame's JSON as an envelope with a `T` body.
pub(crate) fn parse_envelope<'de, T: Deserialize<'de>>(
    json: &'de [u8],
    what: &'static str,
) -> Result<Envelope<T>, Error> {
    serde_json::from_slice(json)
        .map_err(|e| Error::Serialization(format!("file tunnel {what}: {e}").into()))
}

/// `REQUEST_MEDIA_ABILITY` request body, as Studio's `PrinterFileSystem::RequestMediaAbility` builds it.
#[derive(Serialize)]
pub(crate) struct MediaAbilityReq<'a> {
    pub(crate) peer: &'a str,
    pub(crate) api_version: u32,
}

/// `REQUEST_MEDIA_ABILITY` reply body. Studio reads only `storage`.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct MediaAbilityReply {
    #[serde(default)]
    pub(crate) storage: Vec<String>,
}

/// `LIST_INFO` request body, as Studio's `PrinterFileSystem::ListAllFiles` builds it.
#[derive(Serialize)]
pub(crate) struct ListInfoReq<'a> {
    #[serde(rename = "type")]
    pub(crate) kind: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) storage: Option<&'a str>,
    pub(crate) api_version: u32,
    pub(crate) notify: &'a str,
}

/// One entry of a `LIST_INFO` reply.
///
/// Studio requires `name` and `size` and defaults the rest (`ListAllFiles`).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct WireFile {
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) time: u64,
    pub(crate) size: u64,
}

/// `LIST_INFO` reply body.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ListInfoReply {
    #[serde(default)]
    pub(crate) file_lists: Vec<WireFile>,
}

/// `FILE_DOWNLOAD` request body.
///
/// Studio sends `path` when the listing gave one and `file` (the bare name) otherwise
/// (`DownloadNextFile`); a `mem:/` read adds `mem_dl_param_size` (`DownloadRamFile`).
#[derive(Serialize)]
pub(crate) struct DownloadReq<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) path: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) file: Option<&'a str>,
    pub(crate) offset: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) mem_dl_param_size: Option<usize>,
}

/// One `FILE_DOWNLOAD` reply frame's body.
///
/// Studio reads `offset` and `total` from every frame and `size` with a default of 0; the last
/// frame adds `file_md5`. The first frame of a `mem:/` read carries `mem_dl_param_size` instead of
/// file data.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct DownloadReply {
    #[serde(default)]
    pub(crate) offset: u64,
    #[serde(default)]
    pub(crate) total: u64,
    #[serde(default)]
    pub(crate) size: u64,
    #[serde(default)]
    pub(crate) file_md5: Option<String>,
    #[serde(default)]
    pub(crate) mem_dl_param_size: Option<usize>,
}

/// The parameter blob after a `mem:/16` request's JSON, as Studio's `PrinterFileSystem::GetPickImage` builds it.
#[derive(Serialize)]
pub(crate) struct ProjectFileParam<'a> {
    pub(crate) sequence_id: u32,
    pub(crate) version: u32,
    pub(crate) peer_host: &'a str,
    pub(crate) command: &'a str,
    pub(crate) file_rel_path: &'a str,
}

/// `command` of a `mem:/16` read: a member of the running job's `.3mf`.
pub(crate) const MEM_COMMAND_GET_PROJECT_FILE: &str = "get_project_file";
/// `version` of the `mem:/` parameter blob.
pub(crate) const MEM_PARAM_VERSION: u32 = 1;

/// The printer's own blob in the first frame of a `mem:/` read.
///
/// Studio's `DownloadRamFile` fails the read when `result` is missing or `1`, and when `size` is 0.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct MemDownloadStatus {
    #[serde(default)]
    pub(crate) result: Option<i64>,
    #[serde(default)]
    pub(crate) size: Option<u64>,
}

impl MemDownloadStatus {
    /// Whether Studio would continue the read after this status.
    pub(crate) fn accepted(&self) -> bool {
        self.result.is_some_and(|r| r != result::CONTINUE) && self.size != Some(0)
    }
}

/// `TASK_CANCEL` request body, as Studio's `PrinterFileSystem::CancelRequests` builds it.
#[derive(Serialize)]
pub(crate) struct TaskCancelReq<'a> {
    pub(crate) tasks: &'a [u32],
}

/// `SUB_FILE` request body for named members, as Studio's `PrinterFileSystem::UpdateFocusThumbnail2` builds it (without `zip`).
#[derive(Serialize)]
pub(crate) struct SubFileReq<'a> {
    pub(crate) paths: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) storage: Option<&'a str>,
}

/// One `SUB_FILE` reply frame's body.
///
/// Studio reads `path` (the requested `file#member`), `size` (this fragment's length, 0 for a
/// missing member) and `continue` (more fragments of the same member follow).
#[derive(Debug, Default, Deserialize)]
pub(crate) struct SubFileReply {
    #[serde(default)]
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) size: u64,
    #[serde(default, rename = "continue")]
    pub(crate) more_fragments: bool,
}

/// `FILE_DEL` request body, as Studio's `PrinterFileSystem::DeleteFilesContinue` builds it: `paths` when the listing gave them, else `delete` with bare names.
#[derive(Serialize)]
pub(crate) struct FileDelReq<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) paths: Option<&'a [&'a str]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) delete: Option<&'a [&'a str]>,
}

/// The `type` of every upload Studio makes (`PrinterFileSystem::RequestUploadFile`).
pub(crate) const UPLOAD_TYPE_MODEL: &str = "model";

/// `FILE_UPLOAD` init request body, as Studio's `PrinterFileSystem::RequestUploadFile` builds it.
#[derive(Serialize)]
pub(crate) struct UploadInitReq<'a> {
    #[serde(rename = "type")]
    pub(crate) kind: &'a str,
    pub(crate) storage: &'a str,
    pub(crate) path: &'a str,
    pub(crate) total: u64,
}

/// `FILE_UPLOAD` init reply body: the chunk size in KiB and the byte offset to start from.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct UploadInitReply {
    #[serde(default)]
    pub(crate) chunk_size: u64,
    #[serde(default)]
    pub(crate) offset: u64,
}

/// One `FILE_UPLOAD` chunk's request body, as Studio's `PrinterFileSystem::UploadFileTask` builds it.
#[derive(Serialize)]
pub(crate) struct UploadChunkReq<'a> {
    pub(crate) frag_id: u32,
    pub(crate) offset: u64,
    pub(crate) size: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) file_md5: Option<&'a str>,
}

/// Whether `digest` matches the printer's `file_md5`, ignoring case as Studio does.
pub(crate) fn md5_matches(digest: &md5::Digest, file_md5: &str) -> bool {
    format!("{digest:x}").eq_ignore_ascii_case(file_md5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(feature = "std"))]
    use alloc::vec;

    #[test]
    fn header_round_trips_little_endian() {
        let h = FrameHeader {
            payload_len: 0x0102_0304,
            magic: MAGIC_CTRL,
            seq: 7,
        };
        let bytes = h.encode();
        assert_eq!(&bytes[0..4], &[0x04, 0x03, 0x02, 0x01]);
        assert_eq!(&bytes[4..8], &[0x3F, 0x01, 0x02, 0x01]);
        assert_eq!(&bytes[8..12], &[7, 0, 0, 0]);
        assert_eq!(&bytes[12..16], &[0, 0, 0, 0]);
        assert_eq!(FrameHeader::decode(&bytes), h);
    }

    #[test]
    fn decode_ignores_reserved_bytes() {
        let mut bytes = FrameHeader {
            payload_len: 4,
            magic: MAGIC_LOGIN_ACK,
            seq: 1,
        }
        .encode();
        bytes[12] = 0xAA;
        assert_eq!(FrameHeader::decode(&bytes).magic, MAGIC_LOGIN_ACK);
    }

    #[test]
    fn magics_carry_subchannel_and_direction_bytes() {
        // Byte 6 is the subchannel, byte 7 the direction (reference/08 §8.2).
        let b = |m: u32| m.to_le_bytes();
        assert_eq!((b(MAGIC_LOGIN)[2], b(MAGIC_LOGIN)[3]), (0x01, 0x01));
        assert_eq!((b(MAGIC_LOGIN_ACK)[2], b(MAGIC_LOGIN_ACK)[3]), (0x01, 0x00));
        assert_eq!((b(MAGIC_CTRL)[2], b(MAGIC_CTRL)[3]), (0x02, 0x01));
        assert_eq!(
            (b(MAGIC_CTRL_REPLY)[2], b(MAGIC_CTRL_REPLY)[3]),
            (0x02, 0x00)
        );
    }

    #[test]
    fn login_payload_is_two_nul_padded_eight_byte_fields() {
        let p = build_login_payload("12345678").unwrap();
        assert_eq!(&p[..8], b"bblp\0\0\0\0");
        assert_eq!(&p[8..], b"12345678");
    }

    #[test]
    fn login_payload_rejects_a_malformed_access_code() {
        assert!(build_login_payload("1234").is_err());
        assert!(build_login_payload("123456789").is_err());
    }

    #[test]
    fn setup_json_matches_both_reference_clients() {
        // ha-bambulab's `_build_ctrl_setup_json` and open-bamboo-networking's `build_setup_json`
        // both produce exactly this compact string.
        let json = build_setup_json(&setup_pid(0x00ab_cdef)).unwrap();
        assert_eq!(
            core::str::from_utf8(&json).unwrap(),
            r#"{"sequence":0,"mtype":12291,"req":{"t_av":1,"mtype":12289,"peer_t":3,"pid":"00abcdef","ver":"02.03.00.00"}}"#
        );
    }

    #[test]
    fn request_leads_with_mtype_and_omits_the_separator_without_binary() {
        let req = MediaAbilityReq {
            peer: PEER_STUDIO,
            api_version: API_VERSION,
        };
        let wire = encode_request(cmd::REQUEST_MEDIA_ABILITY, 2, &req, &[]).unwrap();
        assert_eq!(
            core::str::from_utf8(&wire).unwrap(),
            r#"{"mtype":12289,"cmdtype":7,"sequence":2,"req":{"peer":"studio","api_version":2}}"#
        );
    }

    #[test]
    fn request_appends_binary_after_the_separator() {
        let req = DownloadReq {
            path: Some("mem:/16"),
            file: None,
            offset: 0,
            mem_dl_param_size: Some(3),
        };
        let wire = encode_request(cmd::FILE_DOWNLOAD, 4, &req, b"abc").unwrap();
        let (json, bin) = split_payload(&wire);
        assert_eq!(
            core::str::from_utf8(json).unwrap(),
            r#"{"mtype":12289,"cmdtype":4,"sequence":4,"req":{"path":"mem:/16","offset":0,"mem_dl_param_size":3}}"#
        );
        assert_eq!(bin, b"abc");
    }

    #[test]
    fn list_request_omits_storage_when_none() {
        let req = ListInfoReq {
            kind: "model",
            storage: None,
            api_version: API_VERSION,
            notify: LIST_NOTIFY_DETAIL,
        };
        let wire = encode_request(cmd::LIST_INFO, 1, &req, &[]).unwrap();
        assert_eq!(
            core::str::from_utf8(&wire).unwrap(),
            r#"{"mtype":12289,"cmdtype":1,"sequence":1,"req":{"type":"model","api_version":2,"notify":"DETAIL"}}"#
        );
    }

    #[test]
    fn split_without_separator_is_all_json() {
        let (json, bin) = split_payload(br#"{"result":0}"#);
        assert_eq!(json, br#"{"result":0}"#);
        assert!(bin.is_empty());
    }

    #[test]
    fn split_keeps_binary_that_itself_contains_the_separator() {
        let payload = b"{\"a\":1}\n\n\x00\n\nrest";
        let (json, bin) = split_payload(payload);
        assert_eq!(json, b"{\"a\":1}");
        assert_eq!(bin, b"\x00\n\nrest");
    }

    #[test]
    fn setup_reply_parses_as_success() {
        let env: Envelope<serde::de::IgnoredAny> = parse_envelope(
            br#"{"mtype":12291,"sequence":0,"result":0,"reply":{}}"#,
            "setup",
        )
        .unwrap();
        assert_eq!(env.sequence, Some(0));
        assert_eq!(env.result, Some(result::SUCCESS));
        assert!(!env.is_notify());
    }

    #[test]
    fn list_reply_parses_studio_shaped_entries() {
        // Shape from open-bamboo-networking's captured round trip and Studio's `ListAllFiles`.
        let json = br#"{"cmdtype":1,"mtype":12289,"reply":{"file_lists":[{"name":"a.3mf","path":"/userdata/model/a.3mf","time":1700000000,"size":42},{"name":"b.mp4","size":7}]},"result":0,"sequence":1}"#;
        let env: Envelope<ListInfoReply> = parse_envelope(json, "LIST_INFO").unwrap();
        let files = env.reply.unwrap().file_lists;
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "/userdata/model/a.3mf");
        assert_eq!(files[0].time, 1_700_000_000);
        assert_eq!(files[1].path, "");
        assert_eq!(files[1].size, 7);
    }

    #[test]
    fn list_entry_without_size_is_rejected_as_studio_rejects_it() {
        let json =
            br#"{"cmdtype":1,"result":0,"sequence":1,"reply":{"file_lists":[{"name":"a.3mf"}]}}"#;
        assert!(parse_envelope::<ListInfoReply>(json, "LIST_INFO").is_err());
    }

    #[test]
    fn ability_reply_reads_storage_and_ignores_the_rest() {
        // X2D reply captured by open-bamboo-networking.
        let json = br#"{"cmdtype":7,"mtype":12289,"reply":{"allow_internal_model_download":true,"api_version":3,"storage":["emmc","udisk"],"upload_storage":["emmc","udisk"]},"result":0,"sequence":2}"#;
        let env: Envelope<MediaAbilityReply> = parse_envelope(json, "ability").unwrap();
        assert_eq!(env.reply.unwrap().storage, vec!["emmc", "udisk"]);
    }

    #[test]
    fn notify_has_no_result() {
        let env: Envelope<serde::de::IgnoredAny> =
            parse_envelope(br#"{"cmdtype":256,"sequence":0,"notify":{}}"#, "notify").unwrap();
        assert!(env.is_notify());
    }

    #[test]
    fn mem_status_follows_studio_acceptance() {
        let st = |json: &[u8]| {
            serde_json::from_slice::<MemDownloadStatus>(json)
                .unwrap()
                .accepted()
        };
        assert!(st(br#"{"result":0}"#));
        assert!(!st(br#"{"result":1}"#));
        assert!(!st(br#"{}"#));
        assert!(!st(br#"{"result":0,"size":0}"#));
    }

    #[test]
    fn mem_param_matches_studio_field_set() {
        let p = ProjectFileParam {
            sequence_id: 1,
            version: MEM_PARAM_VERSION,
            peer_host: PEER_STUDIO,
            command: MEM_COMMAND_GET_PROJECT_FILE,
            file_rel_path: "Metadata/pick_1.png",
        };
        let json = crate::client::serialize(&p).unwrap();
        assert_eq!(
            core::str::from_utf8(&json).unwrap(),
            r#"{"sequence_id":1,"version":1,"peer_host":"studio","command":"get_project_file","file_rel_path":"Metadata/pick_1.png"}"#
        );
    }

    #[test]
    fn md5_comparison_ignores_case() {
        let d = md5::compute(b"abcdefghijklmnopqrstuvwxyz");
        assert!(md5_matches(&d, "c3fcd3d76192e4007dfb496cca67e13b"));
        assert!(md5_matches(&d, "C3FCD3D76192E4007DFB496CCA67E13B"));
        assert!(!md5_matches(&d, "00fcd3d76192e4007dfb496cca67e13b"));
    }
}
