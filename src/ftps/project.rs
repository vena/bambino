//! # Reading plate images from the front of a job's `.3mf`
//!
//! A sliced `.3mf` is a ZIP archive. Bambu Studio and OrcaSlicer both write it in a fixed order
//! (`_BBS_3MF_Exporter::_save_model_to_file` in each): `[Content_Types].xml`, then every plate's
//! thumbnails (`Metadata/plate_N.png`, `plate_N_small.png`, `plate_no_light_N.png`, `top_N.png`,
//! `pick_N.png`), stored uncompressed, then `Metadata/plate_N.json`, and only then the model,
//! G-code and configs. So the plate and pick images sit in the first few tens of KB of a file
//! that can be tens of MB, and [`front_members`] reads them from a prefix of the file
//! ([`FtpsClient::read_front_members`](crate::ftps::FtpsClient::read_front_members) fetches one).
//!
//! Every entry these slicers write sets general-purpose flag bit 3 (sizes in a data descriptor
//! after the data, not in the local header), so the walk can't skip an entry by its header. It
//! finds a stored PNG's end after its `IEND` chunk, and a deflated entry's end by the data
//! descriptor that follows it (its compressed size equals the distance walked, and a ZIP header
//! follows). It never inflates. See `reference/02_ftps.md` [REF-FTPS-3MF-FRONT].

#[cfg(not(feature = "std"))]
use alloc::format;
#[cfg(not(feature = "std"))]
use alloc::string::String;

use crate::error::Error;

/// ZIP local file header signature (`PK\x03\x04`).
const ZIP_LOCAL_HEADER_SIG: [u8; 4] = *b"PK\x03\x04";
/// ZIP central directory header signature (`PK\x01\x02`): the entries are over.
const ZIP_CENTRAL_HEADER_SIG: [u8; 4] = *b"PK\x01\x02";
/// Optional data descriptor signature (`PK\x07\x08`).
const ZIP_DESCRIPTOR_SIG: [u8; 4] = *b"PK\x07\x08";
/// Fixed part of a local file header, before the name and extra field.
const ZIP_LOCAL_HEADER_LEN: usize = 30;
/// General-purpose flag bit 0: the entry is encrypted.
const ZIP_FLAG_ENCRYPTED: u16 = 1 << 0;
/// General-purpose flag bit 3: sizes and CRC are in a data descriptor after the data.
const ZIP_FLAG_DATA_DESCRIPTOR: u16 = 1 << 3;
/// Compression method 0: stored.
const ZIP_METHOD_STORED: u16 = 0;
/// Compression method 8: deflate.
const ZIP_METHOD_DEFLATE: u16 = 8;
/// Extra-field header ID of the Zip64 extended information field.
const ZIP64_EXTRA_ID: u16 = 0x0001;
/// A 32-bit size field set to this defers to the Zip64 extra field.
const ZIP64_SIZE_MARKER: u32 = 0xFFFF_FFFF;

/// The 8-byte PNG file signature.
const PNG_SIGNATURE: [u8; 8] = *b"\x89PNG\r\n\x1a\n";
/// A PNG chunk's length, type and CRC fields: 12 bytes around its data.
const PNG_CHUNK_OVERHEAD: usize = 12;
/// The chunk type that ends a PNG.
const PNG_IEND: [u8; 4] = *b"IEND";

/// Directory holding the plate images and the other members [`front_members`] can reach.
const METADATA_DIR: &str = "Metadata/";
/// The archive's first entry in both slicers' layout.
const CONTENT_TYPES_MEMBER: &str = "[Content_Types].xml";

/// Name of plate `plate`'s image inside the `.3mf`: `Metadata/plate_<plate>.png`.
///
/// `plate` is 1-based, as telemetry's `plate_idx` reports it; `plate_idx` is authoritative over
/// the archive's own plate data (`reference/03_mqtt_telemetry.md`, "`plate_idx`").
#[must_use]
pub fn plate_image_member(plate: u32) -> String {
    format!("Metadata/plate_{plate}.png")
}

/// Name of plate `plate`'s 128×128 thumbnail: `Metadata/plate_<plate>_small.png`.
#[must_use]
pub fn plate_small_image_member(plate: u32) -> String {
    format!("Metadata/plate_{plate}_small.png")
}

/// Name of plate `plate`'s pick image: `Metadata/pick_<plate>.png`.
///
/// Each object on the plate is filled with one flat colour encoding its `identify_id`, the id
/// [`PrinterClient::skip_objects`](crate::client::PrinterClient::skip_objects) takes; decode a
/// pixel with [`pick_identify_id`].
#[must_use]
pub fn pick_image_member(plate: u32) -> String {
    format!("Metadata/pick_{plate}.png")
}

/// Decodes one pick-image pixel, given as `[r, g, b, a]`, into the `identify_id` of the object it belongs to.
///
/// The id is `(b << 16) | (g << 8) | r`, as BambuStudio's `SkipPartCanvas::LoadPickImage` and
/// `GetIdAtImagePt` (via `SkipIdHelper::reverse`) and ha-bambulab's
/// `_identify_objects_in_pick_image` decode it. Returns `None` for background: a transparent
/// pixel (ha-bambulab skips alpha 0) or black (BambuStudio drops alpha and treats id 0 as none).
///
/// Anti-aliased edges hold blended colours that decode to ids no object has. BambuStudio takes
/// each object's majority colour; count pixels per id and keep the dominant ones, or check the
/// ids against telemetry, rather than trusting every decoded pixel. A plate whose pick image
/// holds one id has one object, and skipping objects on it is pointless.
#[must_use]
pub const fn pick_identify_id(rgba: [u8; 4]) -> Option<u32> {
    let [r, g, b, a] = rgba;
    let id = ((b as u32) << 16) | ((g as u32) << 8) | r as u32;
    if a == 0 || id == 0 { None } else { Some(id) }
}

/// Finds the members named in `names` in `prefix`, the first bytes of a `.3mf`, returning each one's bytes or `None`.
///
/// `None` means the member isn't in the front section (`[Content_Types].xml` and the
/// `Metadata/` entries that follow it): a plate number the job doesn't have, or a slicer that
/// wrote no pick image. Members are returned only when stored uncompressed, as the slicers write
/// PNGs; a requested member that is compressed is an error rather than bytes the caller can't
/// use. Stored members are checked against their data descriptor's CRC-32 and sizes.
///
/// The walk stops once every requested member is found, so `prefix` needs to reach only the end
/// of the last of them.
///
/// # Errors
///
/// - [`Error::InvalidArgument`] when `prefix` ends before the walk could settle every name:
///   read a longer prefix.
/// - [`Error::ProtocolViolation`] when the bytes aren't the layout described in the module
///   doc: not a ZIP, an encrypted entry, an entry whose end can't be found without inflating
///   it, a descriptor that disagrees with the data, or a requested member that is compressed.
pub fn front_members<'a, const N: usize>(
    prefix: &'a [u8],
    names: [&str; N],
) -> Result<[Option<&'a [u8]>; N], Error> {
    match walk_front(prefix, &names)? {
        Walk::Done(found) => Ok(found),
        Walk::NeedMore => Err(Error::InvalidArgument(
            "the .3mf prefix ends before the requested members do; read more of the file".into(),
        )),
    }
}

/// Where a front-section walk over a partial buffer got to.
pub(crate) enum Walk<'a, const N: usize> {
    /// Every name is settled: found, or shown absent by the end of the front section.
    Done([Option<&'a [u8]>; N]),
    /// The buffer ends before that could be decided.
    NeedMore,
}

/// Walks local headers from offset 0 until every name in `names` is settled or `buf` runs out.
pub(crate) fn walk_front<'a, const N: usize>(
    buf: &'a [u8],
    names: &[&str; N],
) -> Result<Walk<'a, N>, Error> {
    let mut found: [Option<&'a [u8]>; N] = [None; N];
    let mut pos = 0;
    let mut first = true;
    let mut saw_metadata = false;
    // The front section is over; a section with no `Metadata/` entry isn't the slicers' layout,
    // and "absent" would be a claim about a file this walk doesn't understand.
    let section_end = |found, saw_metadata| {
        if saw_metadata {
            Ok(Walk::Done(found))
        } else {
            Err(violation(
                "the .3mf doesn't open with the slicers' Metadata/ entries",
            ))
        }
    };
    loop {
        if found.iter().all(Option::is_some) {
            return Ok(Walk::Done(found));
        }
        let Some(sig) = buf.get(pos..pos + 4) else {
            return Ok(Walk::NeedMore);
        };
        if sig == ZIP_CENTRAL_HEADER_SIG {
            return section_end(found, saw_metadata);
        }
        if sig != ZIP_LOCAL_HEADER_SIG {
            return Err(violation(
                "no ZIP local header where the previous entry ended",
            ));
        }
        let Some(entry) = LocalHeader::parse(buf, pos)? else {
            return Ok(Walk::NeedMore);
        };
        let is_metadata = entry.name.starts_with(METADATA_DIR.as_bytes());
        if !is_metadata && !(first && entry.name == CONTENT_TYPES_MEMBER.as_bytes()) {
            return section_end(found, saw_metadata);
        }
        first = false;
        saw_metadata |= is_metadata;

        let Some((data_end, next)) = entry.find_end(buf)? else {
            return Ok(Walk::NeedMore);
        };
        if let Some(slot) = names.iter().position(|n| n.as_bytes() == entry.name) {
            if entry.method != ZIP_METHOD_STORED {
                return Err(violation("a requested .3mf member is compressed"));
            }
            if found[slot].is_none() {
                found[slot] = Some(&buf[entry.data_start..data_end]);
            }
        }
        pos = next;
    }
}

/// The parts of a local file header the walk needs.
struct LocalHeader<'a> {
    name: &'a [u8],
    flags: u16,
    method: u16,
    /// CRC-32 and compressed size from the header, meaningful only without a data descriptor.
    crc: u32,
    compressed_size: u64,
    zip64: bool,
    data_start: usize,
}

impl<'a> LocalHeader<'a> {
    /// Parses the header at `pos`; `None` when `buf` ends inside it.
    fn parse(buf: &'a [u8], pos: usize) -> Result<Option<Self>, Error> {
        let Some(fixed) = buf.get(pos..pos + ZIP_LOCAL_HEADER_LEN) else {
            return Ok(None);
        };
        let flags = le_u16(fixed, 6);
        if flags & ZIP_FLAG_ENCRYPTED != 0 {
            return Err(violation("encrypted .3mf entry"));
        }
        let name_len = usize::from(le_u16(fixed, 26));
        let extra_len = usize::from(le_u16(fixed, 28));
        let name_start = pos + ZIP_LOCAL_HEADER_LEN;
        let data_start = name_start + name_len + extra_len;
        let Some(name) = buf.get(name_start..name_start + name_len) else {
            return Ok(None);
        };
        let Some(extra) = buf.get(name_start + name_len..data_start) else {
            return Ok(None);
        };
        let zip64_extra = find_extra(extra, ZIP64_EXTRA_ID);
        let mut compressed_size = u64::from(le_u32(fixed, 18));
        if compressed_size == u64::from(ZIP64_SIZE_MARKER) {
            // Zip64 extra: uncompressed size (8), then compressed size (8).
            match zip64_extra.and_then(|z| z.get(8..16)) {
                Some(field) => compressed_size = le_u64(field, 0),
                None if flags & ZIP_FLAG_DATA_DESCRIPTOR == 0 => {
                    return Err(violation("Zip64 size marker without a Zip64 extra field"));
                }
                None => {}
            }
        }
        Ok(Some(Self {
            name,
            flags,
            method: le_u16(fixed, 8),
            crc: le_u32(fixed, 14),
            compressed_size,
            zip64: zip64_extra.is_some(),
            data_start,
        }))
    }

    /// Finds where this entry's data ends and where the next header starts; `None` when `buf` ends first.
    fn find_end(&self, buf: &[u8]) -> Result<Option<(usize, usize)>, Error> {
        if self.flags & ZIP_FLAG_DATA_DESCRIPTOR == 0 {
            let Some(data_end) = usize::try_from(self.compressed_size)
                .ok()
                .and_then(|size| self.data_start.checked_add(size))
            else {
                return Err(violation(".3mf entry size out of range"));
            };
            if data_end > buf.len() {
                return Ok(None);
            }
            if self.method == ZIP_METHOD_STORED
                && crc32(&buf[self.data_start..data_end]) != self.crc
            {
                return Err(violation(".3mf member fails its CRC-32"));
            }
            return Ok(Some((data_end, data_end)));
        }
        match self.method {
            ZIP_METHOD_STORED => self.find_stored_png_end(buf),
            ZIP_METHOD_DEFLATE => Ok(self.find_deflate_end(buf)),
            _ => Err(violation(
                ".3mf entry uses a compression method other than stored or deflate",
            )),
        }
    }

    /// Size of the data descriptor's two size fields together.
    fn descriptor_sizes_len(&self) -> usize {
        if self.zip64 { 16 } else { 8 }
    }

    /// Reads a size field of the descriptor at `at`.
    fn descriptor_size(&self, buf: &[u8], at: usize) -> u64 {
        if self.zip64 {
            le_u64(buf, at)
        } else {
            u64::from(le_u32(buf, at))
        }
    }

    /// Walks a stored PNG to its `IEND` chunk, then checks the data descriptor after it.
    fn find_stored_png_end(&self, buf: &[u8]) -> Result<Option<(usize, usize)>, Error> {
        let start = self.data_start;
        let Some(sig) = buf.get(start..start + PNG_SIGNATURE.len()) else {
            return Ok(None);
        };
        if sig != PNG_SIGNATURE {
            return Err(violation(
                "stored .3mf entry with a data descriptor is not a PNG, so its end can't be found",
            ));
        }
        let mut pos = start + PNG_SIGNATURE.len();
        let data_end = loop {
            let Some(header) = buf.get(pos..pos + 8) else {
                return Ok(None);
            };
            let len = usize::try_from(u32::from_be_bytes([
                header[0], header[1], header[2], header[3],
            ]))
            .map_err(|_| violation("PNG chunk length out of range"))?;
            let chunk_end = pos
                .checked_add(PNG_CHUNK_OVERHEAD)
                .and_then(|p| p.checked_add(len))
                .ok_or_else(|| violation("PNG chunk length out of range"))?;
            if chunk_end > buf.len() {
                return Ok(None);
            }
            pos = chunk_end;
            if header[4..8] == PNG_IEND {
                break pos;
            }
        };

        // Descriptor: optional signature, CRC-32, compressed size, uncompressed size. The data's
        // CRC is known, which tells a signature apart from a CRC that happens to equal it.
        let crc = crc32(&buf[start..data_end]);
        let mut at = data_end;
        let Some(first) = buf.get(at..at + 4) else {
            return Ok(None);
        };
        if first == ZIP_DESCRIPTOR_SIG && le_u32(first, 0) != crc {
            at += 4;
        }
        let end = at + 4 + self.descriptor_sizes_len();
        if end > buf.len() {
            return Ok(None);
        }
        let len = (data_end - start) as u64;
        if le_u32(buf, at) != crc
            || self.descriptor_size(buf, at + 4) != len
            || self.descriptor_size(buf, at + 4 + self.descriptor_sizes_len() / 2) != len
        {
            return Err(violation(
                ".3mf member disagrees with its data descriptor (CRC-32 or size)",
            ));
        }
        Ok(Some((data_end, end)))
    }

    /// Finds a deflated entry's end by the descriptor after it: its compressed size equals the distance from the data start, and a ZIP header follows it.
    fn find_deflate_end(&self, buf: &[u8]) -> Option<(usize, usize)> {
        let sizes = self.descriptor_sizes_len();
        let mut p = self.data_start;
        // The descriptor without its signature is 4 + `sizes` bytes, then 4 for the next header.
        while p + 4 + sizes + 4 <= buf.len() {
            for sig_len in [4, 0] {
                if sig_len == 4 && buf[p..p + 4] != ZIP_DESCRIPTOR_SIG {
                    continue;
                }
                let crc_at = p + sig_len;
                let end = crc_at + 4 + sizes;
                let Some(next) = buf.get(end..end + 4) else {
                    continue;
                };
                if (next == ZIP_LOCAL_HEADER_SIG || next == ZIP_CENTRAL_HEADER_SIG)
                    && self.descriptor_size(buf, crc_at + 4) == (p - self.data_start) as u64
                {
                    return Some((p, end));
                }
            }
            p += 1;
        }
        None
    }
}

/// Returns the data of the extra-field record with header ID `id`, if present.
fn find_extra(mut extra: &[u8], id: u16) -> Option<&[u8]> {
    while extra.len() >= 4 {
        let len = usize::from(le_u16(extra, 2));
        let data = extra.get(4..4 + len)?;
        if le_u16(extra, 0) == id {
            return Some(data);
        }
        extra = &extra[4 + len..];
    }
    None
}

fn violation(msg: &'static str) -> Error {
    Error::ProtocolViolation(msg.into())
}

fn le_u16(buf: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([buf[at], buf[at + 1]])
}

fn le_u32(buf: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]])
}

fn le_u64(buf: &[u8], at: usize) -> u64 {
    u64::from(le_u32(buf, at)) | (u64::from(le_u32(buf, at + 4)) << 32)
}

/// CRC-32 (IEEE 802.3, reflected polynomial `0xEDB88320`), the checksum ZIP stores per entry.
///
/// Bitwise rather than table-driven: it runs over a few tens of KB once per read, and a 1 KiB
/// table isn't worth carrying for that.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}

#[cfg(test)]
mod tests;
