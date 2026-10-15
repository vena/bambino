#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::*;

/// How an entry's data descriptor is written.
#[derive(Clone, Copy)]
enum Desc {
    /// With the optional `PK\x07\x08` signature, as miniz writes it.
    Signed,
    /// Without the signature.
    Bare,
    /// 8-byte sizes after a Zip64 extra field in the local header.
    Zip64,
}

/// Appends one entry the way the slicers write it: flag bit 3, zeroed header CRC and sizes,
/// data, then a data descriptor. `crc` overrides the descriptor's CRC to corrupt it.
fn entry(out: &mut Vec<u8>, name: &str, method: u16, data: &[u8], desc: Desc, crc: Option<u32>) {
    let extra: &[u8] = match desc {
        Desc::Zip64 => &[
            0x01, 0x00, 0x10, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ],
        _ => &[],
    };
    out.extend_from_slice(b"PK\x03\x04");
    out.extend_from_slice(&20u16.to_le_bytes());
    out.extend_from_slice(&0x0808u16.to_le_bytes());
    out.extend_from_slice(&method.to_le_bytes());
    out.extend_from_slice(&[0; 4]); // time, date
    out.extend_from_slice(&[0; 12]); // crc, sizes: in the descriptor
    out.extend_from_slice(&(name.len() as u16).to_le_bytes());
    out.extend_from_slice(&(extra.len() as u16).to_le_bytes());
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(extra);
    out.extend_from_slice(data);
    if !matches!(desc, Desc::Bare) {
        out.extend_from_slice(b"PK\x07\x08");
    }
    out.extend_from_slice(&crc.unwrap_or_else(|| crc32(data)).to_le_bytes());
    let len = data.len() as u64;
    if matches!(desc, Desc::Zip64) {
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
    } else {
        out.extend_from_slice(&(len as u32).to_le_bytes());
        out.extend_from_slice(&(len as u32).to_le_bytes());
    }
}

/// A minimal PNG: signature, IHDR, one IDAT holding `fill`, IEND. Chunk CRCs aren't checked.
fn png(fill: &[u8]) -> Vec<u8> {
    let mut out = PNG_SIGNATURE.to_vec();
    for (kind, data) in [
        (&b"IHDR"[..], &[0u8; 13][..]),
        (b"IDAT", fill),
        (b"IEND", &[]),
    ] {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        out.extend_from_slice(&[0; 4]);
    }
    out
}

/// Deflate-shaped bytes; the walk never inflates, and the embedded header signature and a
/// false size must not be taken for the entry's end.
const FAKE_DEFLATE: &[u8] = b"\x8d\x8eK\x0e\x83PK\x03\x04\x00\x00\x00\x00PK\x07\x08junk";

/// A one-plate archive in the measured layout, plus where pick_1's descriptor ends.
fn archive(desc: Desc) -> (Vec<u8>, Vec<u8>, Vec<u8>, usize) {
    let plate = png(b"plate pixels");
    let pick = png(b"pick pixels, PK\x03\x04 inside");
    let mut out = Vec::new();
    entry(&mut out, "[Content_Types].xml", 8, FAKE_DEFLATE, desc, None);
    entry(&mut out, "Metadata/plate_1.png", 0, &plate, desc, None);
    entry(
        &mut out,
        "Metadata/plate_1_small.png",
        0,
        &png(b"s"),
        desc,
        None,
    );
    entry(&mut out, "Metadata/top_1.png", 0, &png(b"top"), desc, None);
    entry(&mut out, "Metadata/pick_1.png", 0, &pick, desc, None);
    let pick_end = out.len();
    entry(
        &mut out,
        "Metadata/plate_1.json",
        8,
        FAKE_DEFLATE,
        desc,
        None,
    );
    entry(&mut out, "3D/3dmodel.model", 8, FAKE_DEFLATE, desc, None);
    out.extend_from_slice(b"PK\x01\x02");
    (out, plate, pick, pick_end)
}

const PLATE: &str = "Metadata/plate_1.png";
const PICK: &str = "Metadata/pick_1.png";

#[test]
fn test_front_members_returns_stored_members_byte_for_byte() {
    for desc in [Desc::Signed, Desc::Bare, Desc::Zip64] {
        let (file, plate, pick, _) = archive(desc);
        let [p, k] = front_members(&file, [PLATE, PICK]).expect("walk");
        assert_eq!(p, Some(&plate[..]));
        assert_eq!(k, Some(&pick[..]));
    }
}

#[test]
fn test_front_members_needs_only_up_to_the_last_requested_member() {
    let (file, _, pick, pick_end) = archive(Desc::Signed);
    let [k] = front_members(&file[..pick_end], [PICK]).expect("walk");
    assert_eq!(k, Some(&pick[..]));
    let short = front_members(&file[..pick_end - 1], [PICK]);
    assert!(matches!(short, Err(Error::InvalidArgument(_))), "{short:?}");
}

#[test]
fn test_front_members_reports_absent_members_as_none() {
    // Plate 2 doesn't exist: the front section ends at the first non-`Metadata/` entry.
    let (file, plate, _, _) = archive(Desc::Signed);
    let [p, missing] = front_members(&file, [PLATE, "Metadata/pick_2.png"]).expect("walk");
    assert_eq!(p, Some(&plate[..]));
    assert_eq!(missing, None);
}

#[test]
fn test_front_members_refuses_a_compressed_member() {
    let (file, _, _, _) = archive(Desc::Signed);
    let result = front_members(&file, ["Metadata/plate_1.json"]);
    assert!(
        matches!(result, Err(Error::ProtocolViolation(_))),
        "{result:?}"
    );
}

#[test]
fn test_front_members_checks_the_descriptor_crc() {
    let mut file = Vec::new();
    entry(
        &mut file,
        "[Content_Types].xml",
        8,
        FAKE_DEFLATE,
        Desc::Signed,
        None,
    );
    entry(
        &mut file,
        PLATE,
        0,
        &png(b"x"),
        Desc::Signed,
        Some(0xDEAD_BEEF),
    );
    let result = front_members(&file, [PLATE]);
    assert!(
        matches!(result, Err(Error::ProtocolViolation(_))),
        "{result:?}"
    );
}

#[test]
fn test_front_members_refuses_other_layouts() {
    // A model first: not the slicers' layout, so no answer rather than "absent".
    let mut file = Vec::new();
    entry(
        &mut file,
        "3D/3dmodel.model",
        8,
        FAKE_DEFLATE,
        Desc::Signed,
        None,
    );
    file.extend_from_slice(b"PK\x01\x02");
    let result = front_members(&file, [PLATE]);
    assert!(
        matches!(result, Err(Error::ProtocolViolation(_))),
        "{result:?}"
    );

    let result = front_members(b"not a zip at all", [PLATE]);
    assert!(
        matches!(result, Err(Error::ProtocolViolation(_))),
        "{result:?}"
    );

    // A stored entry with a descriptor that isn't a PNG has no findable end.
    let mut file = Vec::new();
    entry(
        &mut file,
        "Metadata/notes.txt",
        0,
        b"text",
        Desc::Signed,
        None,
    );
    let result = front_members(&file, [PLATE]);
    assert!(
        matches!(result, Err(Error::ProtocolViolation(_))),
        "{result:?}"
    );
}

/// The rest of a sent file after its front section, as both exporters write it: the mesh-less
/// model, the project settings, one checksum per sliced plate, then the first plate's G-code.
fn with_gcode(mut file: Vec<u8>, plates: &[u32]) -> Vec<u8> {
    entry(
        &mut file,
        "3D/3dmodel.model",
        8,
        FAKE_DEFLATE,
        Desc::Signed,
        None,
    );
    entry(
        &mut file,
        "Metadata/project_settings.config",
        8,
        FAKE_DEFLATE,
        Desc::Signed,
        None,
    );
    for plate in plates {
        let name = format!("Metadata/plate_{plate}.gcode.md5");
        entry(&mut file, &name, 8, FAKE_DEFLATE, Desc::Signed, None);
    }
    if let Some(first) = plates.first() {
        let name = format!("Metadata/plate_{first}.gcode");
        entry(&mut file, &name, 8, FAKE_DEFLATE, Desc::Signed, None);
    }
    file.extend_from_slice(b"PK\x01\x02");
    file
}

/// The front section of a two-plate file, as captured on a P1S.
fn two_plate_front() -> Vec<u8> {
    let mut file = Vec::new();
    entry(
        &mut file,
        "[Content_Types].xml",
        8,
        FAKE_DEFLATE,
        Desc::Signed,
        None,
    );
    for name in [
        "Metadata/plate_1.png",
        "Metadata/plate_2.png",
        "Metadata/pick_1.png",
        "Metadata/pick_2.png",
    ] {
        entry(
            &mut file,
            name,
            0,
            &png(name.as_bytes()),
            Desc::Signed,
            None,
        );
    }
    entry(
        &mut file,
        "Metadata/plate_1.json",
        8,
        FAKE_DEFLATE,
        Desc::Signed,
        None,
    );
    entry(
        &mut file,
        "Metadata/plate_2.json",
        8,
        FAKE_DEFLATE,
        Desc::Signed,
        None,
    );
    file
}

#[test]
fn test_gcode_plates_names_the_one_sliced_plate() {
    // Plate 2 sent from a two-plate project: both plates' images, only plate 2's G-code.
    let file = with_gcode(two_plate_front(), &[2]);
    assert_eq!(gcode_plates(&file).expect("walk"), [2]);
    // The images of the named plate come out of the same bytes.
    let [pick] = front_members(&file, ["Metadata/pick_2.png"]).expect("walk");
    assert_eq!(pick, Some(&png(b"Metadata/pick_2.png")[..]));
}

#[test]
fn test_gcode_plates_reports_every_sliced_plate() {
    let file = with_gcode(two_plate_front(), &[1, 2]);
    assert_eq!(gcode_plates(&file).expect("walk"), [1, 2]);
    let file = with_gcode(two_plate_front(), &[]);
    assert!(gcode_plates(&file).expect("walk").is_empty());
}

#[test]
fn test_gcode_plates_needs_the_whole_list() {
    let file = with_gcode(two_plate_front(), &[2]);
    let gcode_at = file
        .windows(b"Metadata/plate_2.gcode".len())
        .rposition(|w| w == b"Metadata/plate_2.gcode")
        .expect("gcode entry")
        - ZIP_LOCAL_HEADER_LEN;
    // The checksum list is complete once the G-code entry's header is readable.
    let ends_at = gcode_at + ZIP_LOCAL_HEADER_LEN + "Metadata/plate_2.gcode".len();
    assert_eq!(gcode_plates(&file[..ends_at]).expect("walk"), [2]);
    let short = gcode_plates(&file[..gcode_at]);
    assert!(matches!(short, Err(Error::InvalidArgument(_))), "{short:?}");
}

#[test]
fn test_plate_of_accepts_only_exact_names() {
    assert_eq!(
        plate_of(b"Metadata/plate_12.gcode.md5", PLATE_GCODE_MD5_SUFFIX),
        Some(12)
    );
    assert_eq!(
        plate_of(b"Metadata/plate_3.gcode", PLATE_GCODE_SUFFIX),
        Some(3)
    );
    // A checksum member is not a G-code member, and neither is anything else.
    assert_eq!(
        plate_of(b"Metadata/plate_3.gcode.md5", PLATE_GCODE_SUFFIX),
        None
    );
    for name in [
        &b"Metadata/plate_.gcode"[..],
        b"Metadata/plate_0.gcode",
        b"Metadata/plate_-1.gcode",
        b"Metadata/plate_1a.gcode",
        b"Metadata/plate_99999999999.gcode",
        b"x/Metadata/plate_1.gcode",
    ] {
        assert_eq!(plate_of(name, PLATE_GCODE_SUFFIX), None, "{name:?}");
    }
    assert_eq!(plate_of_gcode_param("Metadata/plate_2.gcode"), Some(2));
    assert_eq!(plate_of_gcode_param("/data/Metadata/plate_2.gcode"), None);
}

#[test]
fn test_member_names() {
    assert_eq!(plate_image_member(2), "Metadata/plate_2.png");
    assert_eq!(plate_small_image_member(1), "Metadata/plate_1_small.png");
    assert_eq!(pick_image_member(3), "Metadata/pick_3.png");
}

#[test]
fn test_pick_identify_id() {
    // R is the low byte, B the high one.
    assert_eq!(pick_identify_id([82, 0, 0, 255]), Some(82));
    assert_eq!(pick_identify_id([1, 2, 3, 255]), Some(0x03_02_01));
    assert_eq!(pick_identify_id([0, 0, 0, 255]), None);
    assert_eq!(pick_identify_id([5, 6, 7, 0]), None);
}

#[test]
fn test_crc32_check_value() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32(b""), 0);
}
