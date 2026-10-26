//! # Reading a plate's filament list from a `.3mf`'s `Metadata/slice_info.config`
//!
//! The filaments a plate uses, with their type, colour and preset id, are in the archive's
//! `Metadata/slice_info.config`, which both slicers write near the **end** of the file
//! (`reference/02_ftps.md` [REF-FTPS-3MF-FRONT]), so the front-section reads in the parent module
//! can't reach it. [`read_plate_filaments`] finds it through the ZIP's end of central directory
//! instead: the file's tail, then that member's central directory entry, then its local header,
//! then the member's data, which is deflated (`_BBS_3MF_Exporter` writes it with
//! `mz_zip_writer_add_mem(..., MZ_DEFAULT_COMPRESSION)`, `bbs_3mf.cpp`). It reads from an
//! [`UploadSource`], so a file the caller holds on an SD card is read in a few small pieces
//! rather than loaded.
//!
//! Only the file the caller holds locally is read this way. Reading it from the printer would
//! mean downloading the whole file over FTPS, which has no `REST`; the file tunnel is #636.
//!
//! Zip64 is refused, not parsed: a `.3mf` under 4 GiB never needs it, and the printer's own
//! upload limit is far below that.

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::{
    ZIP_CENTRAL_HEADER_SIG, ZIP_FLAG_ENCRYPTED, ZIP_LOCAL_HEADER_LEN, ZIP_LOCAL_HEADER_SIG,
    ZIP_METHOD_DEFLATE, ZIP_METHOD_STORED, ZIP64_SIZE_MARKER, crc32, le_u16, le_u32, violation,
};
use crate::error::Error;
use crate::upload::{UploadSource, read_next};

/// The member that lists each plate's filaments.
const SLICE_INFO_MEMBER: &str = "Metadata/slice_info.config";

/// Largest `slice_info.config` read, compressed or inflated, in bytes.
///
/// A member claiming more is refused before anything is allocated for it. A real one is a few KB
/// per plate (the objects, filaments and warnings of each), so 2 MiB leaves room for a project
/// with 100 plates while bounding the allocation an embedded consumer makes for a hostile file.
pub(crate) const SLICE_INFO_MAX_BYTES: usize = 2 * 1024 * 1024;

/// ZIP end of central directory signature (`PK\x05\x06`).
const ZIP_EOCD_SIG: [u8; 4] = *b"PK\x05\x06";
/// Zip64 end of central directory locator signature (`PK\x06\x07`).
const ZIP64_LOCATOR_SIG: [u8; 4] = *b"PK\x06\x07";
/// Length of the end of central directory record before its comment.
const ZIP_EOCD_LEN: usize = 22;
/// Longest archive comment, since its length is a `u16`.
const ZIP_MAX_COMMENT: usize = u16::MAX as usize;
/// Fixed part of a central directory entry, before the name, extra field and comment.
const ZIP_CENTRAL_ENTRY_LEN: usize = 46;
/// A 16-bit end-of-central-directory field set to this defers to Zip64.
const ZIP64_COUNT_MARKER: u16 = 0xFFFF;
/// Bytes fetched at a time while walking the central directory and reading headers.
const READ_WINDOW: usize = 4096;

/// Which extruder the slicer assigned a project filament to, as `filament_maps` encodes it.
const FILAMENT_MAP_DEPUTY: u8 = 1;

/// One filament a plate uses, as `slice_info.config` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlateFilament {
    /// The project filament index, **1-based**, as the file writes it.
    ///
    /// This is the `filament_id` [`build_ams_mapping2`](crate::ams::build_ams_mapping2) takes, and
    /// the position (minus one) of this filament's entry in an `ams_mapping2` array. BambuStudio
    /// holds it 0-based internally and writes `id + 1`.
    pub id: u32,
    /// The material, such as `"PLA"` or `"PETG"`.
    pub filament_type: String,
    /// The colour as the file writes it, `"#RRGGBB"` (or `"#RRGGBBAA"`).
    pub color: String,
    /// The filament preset's id, such as `"GFA00"`; empty for a custom preset.
    ///
    /// BambuStudio's `filament_id`, which it matches against a tray's `tray_info_idx`. The file
    /// has no `setting_id`, so BambuStudio's finer preset match never fires for a file's filament.
    pub tray_info_idx: String,
}

/// The filaments one plate uses, and the extruder each project filament was assigned to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlateFilaments {
    /// The plate, 1-based, as telemetry's `plate_idx` reports it.
    pub plate: u32,
    /// The filaments the plate prints with, in file order. A project filament the plate doesn't
    /// use isn't listed.
    pub filaments: Vec<PlateFilament>,
    /// The extruder of every **project** filament, indexed by `id - 1`: `1` is the left (deputy)
    /// extruder and `2` the right (main) one. `None` when the file records none.
    ///
    /// Only a dual-nozzle project sets it to anything but a single extruder.
    pub filament_maps: Option<Vec<u8>>,
}

impl PlateFilaments {
    /// The extruder value of filament `id` (1-based), or `None` when the file records none for it.
    #[must_use]
    pub fn extruder_of(&self, id: u32) -> Option<u8> {
        let index = usize::try_from(id.checked_sub(1)?).ok()?;
        self.filament_maps.as_deref()?.get(index).copied()
    }

    /// Whether filament `id` (1-based) is assigned to the left (deputy) extruder.
    #[must_use]
    pub fn is_deputy(&self, id: u32) -> bool {
        self.extruder_of(id) == Some(FILAMENT_MAP_DEPUTY)
    }
}

/// Reads plate `plate`'s filaments from the `.3mf` in `source`, found through the ZIP's central directory.
///
/// `plate` is 1-based. Reads the file's last few KB, the central directory in 4 KiB pieces, and
/// then the one member, inflating it with a cap of 2 MiB.
///
/// # Errors
///
/// - [`Error::InvalidArgument`] when `slice_info.config` has no plate `plate`, or `source` fails
///   its own length.
/// - [`Error::ProtocolViolation`] when the bytes aren't a ZIP this reader follows: no end of
///   central directory, a Zip64 or multi-disk archive, an encrypted or oversized member, a
///   method other than stored or deflate, a failed inflate or CRC-32, or no
///   `Metadata/slice_info.config` at all, or XML it can't read.
///
/// Whatever `source`'s own read fails with.
pub async fn read_plate_filaments(
    mut source: impl UploadSource,
    plate: u32,
) -> Result<PlateFilaments, Error> {
    let bytes = read_member(&mut source, SLICE_INFO_MEMBER).await?;
    let text = core::str::from_utf8(&bytes)
        .map_err(|_| violation("slice_info.config is not valid UTF-8"))?;
    parse_slice_info(text, plate)
}

/// Parses plate `plate`'s filaments out of an already inflated `slice_info.config`.
///
/// Reads the `<plate>` blocks BambuStudio's exporter writes: `<metadata key="index" value="N"/>`
/// names the plate, `<metadata key="filament_maps" value="1 2 ..."/>` gives the extruder of each
/// project filament, and each `<filament id=".." tray_info_idx=".." type=".." color=".."/>` is a
/// filament the plate uses. `<mixed_filament>` elements are not filaments here. The XML entities
/// `&amp; &lt; &gt; &quot; &apos;` are unescaped in attribute values; any other `&` is kept.
///
/// # Errors
///
/// - [`Error::InvalidArgument`] when no plate has index `plate`.
/// - [`Error::ProtocolViolation`] for markup it can't scan, a `<filament>` with no valid positive
///   `id`, or a `filament_maps` with a value that isn't a number.
pub fn parse_slice_info(text: &str, plate: u32) -> Result<PlateFilaments, Error> {
    let mut in_plate = false;
    let mut current = PlateFilaments {
        plate: 0,
        filaments: Vec::new(),
        filament_maps: None,
    };
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        rest = &rest[open..];
        if let Some(after) = rest.strip_prefix("<!--") {
            let end = after
                .find("-->")
                .ok_or_else(|| violation("unterminated XML comment in slice_info.config"))?;
            rest = &after[end + 3..];
            continue;
        }
        let end =
            tag_end(rest).ok_or_else(|| violation("unterminated XML tag in slice_info.config"))?;
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        if let Some(name) = tag.strip_prefix('/') {
            if name.trim() == "plate" && in_plate {
                if current.plate == plate {
                    return Ok(current);
                }
                in_plate = false;
            }
            continue;
        }
        let tag = tag.strip_suffix('/').unwrap_or(tag);
        let name_end = tag
            .find(|c: char| c.is_ascii_whitespace())
            .unwrap_or(tag.len());
        let (name, attrs) = tag.split_at(name_end);
        match name {
            "plate" => {
                in_plate = true;
                current = PlateFilaments {
                    plate: 0,
                    filaments: Vec::new(),
                    filament_maps: None,
                };
            }
            "metadata" if in_plate => {
                let attrs = Attributes::new(attrs)?;
                match attrs.get("key").as_deref() {
                    Some("index") => {
                        current.plate = attrs
                            .get("value")
                            .and_then(|v| v.trim().parse().ok())
                            .ok_or_else(|| violation("plate index is not a number"))?;
                    }
                    Some("filament_maps") => {
                        current.filament_maps = parse_filament_maps(&attrs.get("value"))?;
                    }
                    _ => {}
                }
            }
            "filament" if in_plate => {
                let attrs = Attributes::new(attrs)?;
                let id = attrs
                    .get("id")
                    .and_then(|v| v.trim().parse::<u32>().ok())
                    .filter(|&id| id > 0)
                    .ok_or_else(|| violation("<filament> without a positive id"))?;
                current.filaments.push(PlateFilament {
                    id,
                    filament_type: attrs.get("type").unwrap_or_default(),
                    color: attrs.get("color").unwrap_or_default(),
                    tray_info_idx: attrs.get("tray_info_idx").unwrap_or_default(),
                });
            }
            _ => {}
        }
    }
    Err(Error::InvalidArgument(
        format!("slice_info.config has no plate {plate}").into(),
    ))
}

/// Index of the `>` that ends the tag starting at `s[0] == '<'`, skipping any inside a quoted value.
fn tag_end(s: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in s.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '>' => return Some(i),
            None => {}
        }
    }
    None
}

/// Parses a `filament_maps` value, space-separated extruder numbers; an empty value is `None`.
fn parse_filament_maps(value: &Option<String>) -> Result<Option<Vec<u8>>, Error> {
    let Some(value) = value else {
        return Ok(None);
    };
    let maps = value
        .split_ascii_whitespace()
        .map(|n| {
            n.parse::<u8>()
                .map_err(|_| violation("filament_maps holds a value that is not a number"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(if maps.is_empty() { None } else { Some(maps) })
}

/// The attributes of one tag, scanned from its text after the element name.
struct Attributes<'a> {
    pairs: Vec<(&'a str, &'a str)>,
}

impl<'a> Attributes<'a> {
    /// Scans `name="value"` and `name='value'` pairs.
    fn new(mut s: &'a str) -> Result<Self, Error> {
        let mut pairs = Vec::new();
        loop {
            s = s.trim_start();
            if s.is_empty() {
                return Ok(Self { pairs });
            }
            let eq = s
                .find('=')
                .ok_or_else(|| violation("XML attribute without a value in slice_info.config"))?;
            let name = s[..eq].trim();
            let after = s[eq + 1..].trim_start();
            let quote = after
                .chars()
                .next()
                .filter(|c| *c == '"' || *c == '\'')
                .ok_or_else(|| violation("unquoted XML attribute in slice_info.config"))?;
            let body = &after[1..];
            let close = body
                .find(quote)
                .ok_or_else(|| violation("unterminated XML attribute in slice_info.config"))?;
            pairs.push((name, &body[..close]));
            s = &body[close + 1..];
        }
    }

    /// The unescaped value of attribute `name`, if present.
    fn get(&self, name: &str) -> Option<String> {
        self.pairs
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| unescape(v))
    }
}

/// Unescapes `&amp; &lt; &gt; &quot; &apos;`; any other `&` stays as written.
fn unescape(value: &str) -> String {
    const ENTITIES: [(&str, char); 5] = [
        ("&amp;", '&'),
        ("&lt;", '<'),
        ("&gt;", '>'),
        ("&quot;", '"'),
        ("&apos;", '\''),
    ];
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        match ENTITIES.iter().find(|(entity, _)| rest.starts_with(entity)) {
            Some((entity, c)) => {
                out.push(*c);
                rest = &rest[entity.len()..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A buffered view over a source, so the central directory's many small reads are a few large ones.
struct Window {
    start: u64,
    buf: Vec<u8>,
}

impl Window {
    fn new() -> Self {
        Self {
            start: 0,
            buf: Vec::new(),
        }
    }

    /// Returns `len` bytes at `offset`, refilling the window when they aren't in it.
    async fn slice<S: UploadSource>(
        &mut self,
        source: &mut S,
        offset: u64,
        len: usize,
    ) -> Result<&[u8], Error> {
        let total = source.len();
        let end = offset
            .checked_add(len as u64)
            .filter(|&end| end <= total)
            .ok_or_else(|| violation("a ZIP structure points past the end of the file"))?;
        let held = offset >= self.start && end <= self.start + self.buf.len() as u64;
        if !held {
            let remaining = usize::try_from(total - offset).unwrap_or(usize::MAX);
            self.buf.clear();
            self.buf.resize(len.max(READ_WINDOW).min(remaining), 0);
            self.start = offset;
            read_exact_at(source, offset, &mut self.buf).await?;
        }
        let at = (offset - self.start) as usize;
        Ok(&self.buf[at..at + len])
    }
}

/// Fills `buf` from `source` at `offset`, across short reads.
async fn read_exact_at<S: UploadSource>(
    source: &mut S,
    offset: u64,
    buf: &mut [u8],
) -> Result<(), Error> {
    let mut done = 0;
    while done < buf.len() {
        let n = read_next(source, offset + done as u64, &mut buf[done..]).await?;
        if n == 0 {
            return Err(violation("the .3mf source ended before its length"));
        }
        done += n;
    }
    Ok(())
}

/// Where a member's data sits, from its central directory entry.
struct MemberLocation {
    method: u16,
    crc: u32,
    compressed_size: u32,
    uncompressed_size: u32,
    local_header_offset: u32,
}

/// Returns the bytes of the archive member `name`, inflated if it is deflated and checked against its CRC-32.
async fn read_member<S: UploadSource>(source: &mut S, name: &str) -> Result<Vec<u8>, Error> {
    let len = source.len();
    let mut window = Window::new();

    // End of central directory: the last record whose comment runs exactly to the end of file.
    let tail_len = usize::try_from(len).map_or(ZIP_EOCD_LEN + ZIP_MAX_COMMENT, |l| {
        l.min(ZIP_EOCD_LEN + ZIP_MAX_COMMENT)
    });
    if tail_len < ZIP_EOCD_LEN {
        return Err(violation("the file is too short to be a ZIP archive"));
    }
    let tail_start = len - tail_len as u64;
    let tail = window.slice(source, tail_start, tail_len).await?.to_vec();
    let eocd = (0..=tail_len - ZIP_EOCD_LEN)
        .rev()
        .find(|&at| {
            tail[at..at + 4] == ZIP_EOCD_SIG
                && at + ZIP_EOCD_LEN + usize::from(le_u16(&tail, at + 20)) == tail_len
        })
        .ok_or_else(|| violation("no ZIP end of central directory: not a .3mf archive"))?;
    let record = &tail[eocd..eocd + ZIP_EOCD_LEN];
    let entries = le_u16(record, 10);
    let cd_size = le_u32(record, 12);
    let cd_offset = le_u32(record, 16);
    if (eocd >= 20 && tail[eocd - 20..eocd - 16] == ZIP64_LOCATOR_SIG)
        || entries == ZIP64_COUNT_MARKER
        || cd_size == ZIP64_SIZE_MARKER
        || cd_offset == ZIP64_SIZE_MARKER
    {
        return Err(violation("a Zip64 .3mf archive is not supported"));
    }
    if le_u16(record, 4) != 0 || le_u16(record, 6) != 0 || le_u16(record, 8) != entries {
        return Err(violation("a multi-disk .3mf archive is not supported"));
    }
    let eocd_at = tail_start + eocd as u64;
    if u64::from(cd_offset) + u64::from(cd_size) > eocd_at {
        return Err(violation(
            "the ZIP central directory overlaps the end of central directory record",
        ));
    }

    // Central directory: find the member's entry.
    let location = find_central_entry(
        source,
        &mut window,
        u64::from(cd_offset),
        u64::from(cd_size),
        entries,
        name.as_bytes(),
    )
    .await?
    .ok_or_else(|| violation("the .3mf has no Metadata/slice_info.config"))?;

    if location.compressed_size == ZIP64_SIZE_MARKER
        || location.uncompressed_size == ZIP64_SIZE_MARKER
        || location.local_header_offset == ZIP64_SIZE_MARKER
    {
        return Err(violation("a Zip64 .3mf archive is not supported"));
    }
    if location.compressed_size as usize > SLICE_INFO_MAX_BYTES
        || location.uncompressed_size as usize > SLICE_INFO_MAX_BYTES
    {
        return Err(violation("slice_info.config is larger than the read cap"));
    }

    // Local header: only to learn where the data starts. Its sizes are zero when the entry has a
    // data descriptor, so the central directory's are the ones used.
    let local_at = u64::from(location.local_header_offset);
    let fixed: [u8; ZIP_LOCAL_HEADER_LEN] = window
        .slice(source, local_at, ZIP_LOCAL_HEADER_LEN)
        .await?
        .try_into()
        .map_err(|_| violation("short ZIP local header"))?;
    if fixed[..4] != ZIP_LOCAL_HEADER_SIG {
        return Err(violation(
            "no ZIP local header where the central directory points",
        ));
    }
    let data_at = local_at
        + ZIP_LOCAL_HEADER_LEN as u64
        + u64::from(le_u16(&fixed, 26))
        + u64::from(le_u16(&fixed, 28));
    if data_at + u64::from(location.compressed_size) > u64::from(cd_offset) {
        return Err(violation(
            "the member's data runs into the central directory",
        ));
    }
    let mut data = vec![0u8; location.compressed_size as usize];
    read_exact_at(source, data_at, &mut data).await?;

    let bytes = match location.method {
        ZIP_METHOD_STORED => {
            if location.compressed_size != location.uncompressed_size {
                return Err(violation("a stored member's two sizes disagree"));
            }
            data
        }
        ZIP_METHOD_DEFLATE => {
            miniz_oxide::inflate::decompress_to_vec_with_limit(&data, SLICE_INFO_MAX_BYTES)
                .map_err(|_| violation("slice_info.config fails to inflate"))?
        }
        _ => {
            return Err(violation(
                "slice_info.config uses a compression method other than stored or deflate",
            ));
        }
    };
    if bytes.len() != location.uncompressed_size as usize || crc32(&bytes) != location.crc {
        return Err(violation("slice_info.config fails its size or CRC-32"));
    }
    Ok(bytes)
}

/// Walks the central directory for an entry named `name`, ignoring ASCII case as BambuStudio does.
async fn find_central_entry<S: UploadSource>(
    source: &mut S,
    window: &mut Window,
    cd_offset: u64,
    cd_size: u64,
    entries: u16,
    name: &[u8],
) -> Result<Option<MemberLocation>, Error> {
    let mut pos = 0u64;
    for _ in 0..entries {
        if pos + ZIP_CENTRAL_ENTRY_LEN as u64 > cd_size {
            return Err(violation("the ZIP central directory ends inside an entry"));
        }
        let fixed: [u8; ZIP_CENTRAL_ENTRY_LEN] = window
            .slice(source, cd_offset + pos, ZIP_CENTRAL_ENTRY_LEN)
            .await?
            .try_into()
            .map_err(|_| violation("short ZIP central directory entry"))?;
        if fixed[..4] != ZIP_CENTRAL_HEADER_SIG {
            return Err(violation("no ZIP central directory entry where one is due"));
        }
        let name_len = usize::from(le_u16(&fixed, 28));
        let entry_len = ZIP_CENTRAL_ENTRY_LEN as u64
            + name_len as u64
            + u64::from(le_u16(&fixed, 30))
            + u64::from(le_u16(&fixed, 32));
        if pos + entry_len > cd_size {
            return Err(violation("the ZIP central directory ends inside an entry"));
        }
        if name_len == name.len() {
            let entry_name = window
                .slice(
                    source,
                    cd_offset + pos + ZIP_CENTRAL_ENTRY_LEN as u64,
                    name_len,
                )
                .await?;
            if entry_name.eq_ignore_ascii_case(name) {
                if le_u16(&fixed, 8) & ZIP_FLAG_ENCRYPTED != 0 {
                    return Err(violation("encrypted .3mf entry"));
                }
                return Ok(Some(MemberLocation {
                    method: le_u16(&fixed, 10),
                    crc: le_u32(&fixed, 16),
                    compressed_size: le_u32(&fixed, 20),
                    uncompressed_size: le_u32(&fixed, 24),
                    local_header_offset: le_u32(&fixed, 42),
                }));
            }
        }
        pos += entry_len;
    }
    Ok(None)
}

#[cfg(test)]
mod tests;
