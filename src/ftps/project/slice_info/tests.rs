#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::*;

const XML: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<config>
  <!-- written by the slicer -->
  <header>
    <header_item key="X-BBL-Client-Type" value="slicer"/>
  </header>
  <plate>
    <metadata key="index" value="1"/>
    <metadata key="filament_maps" value="1 1 1"/>
    <object identify_id="5" name="Cube &amp; co &gt; x" skipped="false" />
    <filament id="1" tray_info_idx="GFA00" type="PLA" color="#FFFFFF" used_m="1.0" used_g="3.0" group_id="0" />
    <filament id="3" tray_info_idx="" type="PETG" color="#FF0000" used_m="2.0" used_g="6.0" group_id="0"/>
    <mixed_filament id="5" type="PLA" color="#000000" components="1,2"/>
  </plate>
  <plate>
    <metadata key="index" value="2"/>
    <metadata key="filament_maps" value="2 1 2 2"/>
    <filament id="2" tray_info_idx="GFB&amp;1" type='PA-CF' color="#00FF00FF" />
    <filament id="4" tray_info_idx="GFL99" type="PLA &quot;Silk&quot;" color="#0000FF" />
  </plate>
</config>
"##;

#[test]
fn test_parse_single_nozzle_plate() {
    let plate = parse_slice_info(XML, 1).expect("plate 1");
    assert_eq!(plate.plate, 1);
    assert_eq!(plate.filament_maps.as_deref(), Some(&[1, 1, 1][..]));
    assert_eq!(plate.filaments.len(), 2, "mixed_filament is not a filament");
    assert_eq!(
        plate.filaments[0],
        PlateFilament {
            id: 1,
            filament_type: "PLA".into(),
            color: "#FFFFFF".into(),
            tray_info_idx: "GFA00".into(),
        }
    );
    // ids stay 1-based and gaps stay gaps: filament 2 isn't used on this plate.
    assert_eq!(plate.filaments[1].id, 3);
    assert_eq!(plate.filaments[1].tray_info_idx, "");
}

#[test]
fn test_parse_dual_nozzle_plate_and_unescape() {
    let plate = parse_slice_info(XML, 2).expect("plate 2");
    assert_eq!(plate.filament_maps.as_deref(), Some(&[2, 1, 2, 2][..]));
    assert_eq!(plate.extruder_of(2), Some(1));
    assert!(plate.is_deputy(2) && !plate.is_deputy(1));
    assert_eq!(plate.extruder_of(0), None);
    assert_eq!(plate.extruder_of(5), None);
    assert_eq!(plate.filaments[0].tray_info_idx, "GFB&1");
    assert_eq!(plate.filaments[0].filament_type, "PA-CF");
    assert_eq!(plate.filaments[0].color, "#00FF00FF");
    assert_eq!(plate.filaments[1].filament_type, "PLA \"Silk\"");
}

#[test]
fn test_parse_missing_plate_is_an_argument_error() {
    let result = parse_slice_info(XML, 3);
    assert!(
        matches!(result, Err(Error::InvalidArgument(_))),
        "{result:?}"
    );
    // A plate with the right number but no closing tag is not returned.
    let open = "<config><plate><metadata key=\"index\" value=\"1\"/>";
    assert!(matches!(
        parse_slice_info(open, 1),
        Err(Error::InvalidArgument(_))
    ));
}

#[test]
fn test_parse_without_filament_maps_has_none() {
    let xml = r##"<config><plate><metadata key="index" value="1"/>
        <metadata key="filament_maps" value=""/>
        <filament id="1" type="PLA" color="#FFFFFF"/></plate></config>"##;
    let plate = parse_slice_info(xml, 1).expect("plate");
    assert_eq!(plate.filament_maps, None);
    assert_eq!(plate.filaments[0].tray_info_idx, "");
}

#[test]
fn test_parse_refuses_malformed_markup() {
    for bad in [
        r#"<config><plate><metadata key="index" value="1"/><filament id="0" type="PLA"/></plate></config>"#,
        r#"<config><plate><metadata key="index" value="1"/><filament type="PLA"/></plate></config>"#,
        r#"<config><plate><metadata key="index" value="1"/><metadata key="filament_maps" value="1 x"/></plate></config>"#,
        r#"<config><plate><metadata key="index" value="x"/></plate></config>"#,
        r#"<config><plate><filament id="1" type="PLA></plate></config>"#,
        r#"<config><plate><filament id=1/></plate></config>"#,
    ] {
        let result = parse_slice_info(bad, 1);
        assert!(
            matches!(result, Err(Error::ProtocolViolation(_))),
            "{bad}: {result:?}"
        );
    }
}

#[test]
fn test_unescape_keeps_unknown_entities() {
    assert_eq!(unescape("a &amp;&lt;&gt;&quot;&apos; b"), "a &<>\"' b");
    assert_eq!(unescape("&#35;x & &amp"), "&#35;x & &amp");
}

/// Builds a ZIP the way the slicers do, flag bit 3 with zeroed local sizes, plus a central
/// directory and an end record carrying `comment`.
fn zip(members: &[(&str, u16, &[u8])], comment: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for &(name, method, raw) in members {
        let data = if method == ZIP_METHOD_DEFLATE {
            miniz_oxide::deflate::compress_to_vec(raw, 6)
        } else {
            raw.to_vec()
        };
        let crc = crc32(raw);
        let offset = out.len() as u32;
        out.extend_from_slice(b"PK\x03\x04");
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0x0808u16.to_le_bytes());
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&[0; 12]);
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&data);
        out.extend_from_slice(b"PK\x07\x08");
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(raw.len() as u32).to_le_bytes());

        central.extend_from_slice(b"PK\x01\x02");
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&0x0808u16.to_le_bytes());
        central.extend_from_slice(&method.to_le_bytes());
        central.extend_from_slice(&[0; 4]);
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&[0; 2 + 2 + 2 + 2 + 4]); // extra, comment, disk, attrs
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let cd_offset = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(b"PK\x05\x06");
    out.extend_from_slice(&[0; 4]); // disk numbers
    out.extend_from_slice(&(members.len() as u16).to_le_bytes());
    out.extend_from_slice(&(members.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&(comment.len() as u16).to_le_bytes());
    out.extend_from_slice(comment);
    out
}

/// A source that returns at most `chunk` bytes per read, as an SD card driver may.
struct Chunked<'a> {
    bytes: &'a [u8],
    chunk: usize,
}

impl UploadSource for Chunked<'_> {
    fn len(&self) -> u64 {
        self.bytes.len() as u64
    }

    async fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, Error> {
        let start = offset as usize;
        let n = buf.len().min(self.chunk).min(self.bytes.len() - start);
        buf[..n].copy_from_slice(&self.bytes[start..start + n]);
        Ok(n)
    }
}

/// A `.3mf`-shaped archive: an image, the config member, then the model after it.
fn three_mf(method: u16, comment: &[u8]) -> Vec<u8> {
    zip(
        &[
            ("[Content_Types].xml", 8, b"<Types/>"),
            ("Metadata/plate_1.png", 0, b"\x89PNG not really"),
            ("Metadata/slice_info.config", method, XML.as_bytes()),
            ("Metadata/plate_1.gcode", 8, &[7u8; 5000]),
        ],
        comment,
    )
}

#[tokio::test]
async fn test_read_plate_filaments_from_a_deflated_member() {
    let file = three_mf(ZIP_METHOD_DEFLATE, b"");
    let plate = read_plate_filaments(&file[..], 2).await.expect("read");
    assert_eq!(plate, parse_slice_info(XML, 2).unwrap());
}

#[tokio::test]
async fn test_read_plate_filaments_from_a_stored_member() {
    let file = three_mf(ZIP_METHOD_STORED, b"");
    let plate = read_plate_filaments(&file[..], 1).await.expect("read");
    assert_eq!(plate.filaments.len(), 2);
}

#[tokio::test]
async fn test_read_plate_filaments_with_an_archive_comment_and_short_reads() {
    // A comment holding the end signature must not be taken for the end record.
    let comment = b"PK\x05\x06 not the record";
    let file = three_mf(ZIP_METHOD_DEFLATE, comment);
    for chunk in [1, 7, 4096] {
        let source = Chunked {
            bytes: &file,
            chunk,
        };
        let plate = read_plate_filaments(source, 1).await.expect("read");
        assert_eq!(
            plate.filament_maps.as_deref(),
            Some(&[1, 1, 1][..]),
            "{chunk}"
        );
    }
    // A caller's source passed as `&mut`.
    let mut source = &file[..];
    assert!(read_plate_filaments(&mut source, 1).await.is_ok());
}

#[tokio::test]
async fn test_read_plate_filaments_with_a_comment_past_the_first_window() {
    // The end record sits more than the first 4 KiB tail read from the end of the file.
    let comment = vec![b'x'; READ_WINDOW * 3];
    let file = three_mf(ZIP_METHOD_DEFLATE, &comment);
    let plate = read_plate_filaments(&file[..], 2).await.expect("read");
    assert_eq!(plate, parse_slice_info(XML, 2).unwrap());
}

#[tokio::test]
async fn test_read_plate_filaments_missing_plate_and_missing_member() {
    let file = three_mf(ZIP_METHOD_DEFLATE, b"");
    let result = read_plate_filaments(&file[..], 9).await;
    assert!(
        matches!(result, Err(Error::InvalidArgument(_))),
        "{result:?}"
    );

    let no_member = zip(&[("Metadata/plate_1.png", 0, b"x")], b"");
    let result = read_plate_filaments(&no_member[..], 1).await;
    assert!(
        matches!(result, Err(Error::ProtocolViolation(_))),
        "{result:?}"
    );
}

#[tokio::test]
async fn test_read_plate_filaments_refuses_bad_archives() {
    let file = three_mf(ZIP_METHOD_DEFLATE, b"");
    // Truncated: the end record is gone.
    for cut in [0, 10, file.len() - 1, file.len() / 2] {
        let result = read_plate_filaments(&file[..cut], 1).await;
        assert!(
            matches!(result, Err(Error::ProtocolViolation(_))),
            "cut {cut}: {result:?}"
        );
    }
    // Not a ZIP at all.
    let junk = [0x5Au8; 300];
    let result = read_plate_filaments(&junk[..], 1).await;
    assert!(
        matches!(result, Err(Error::ProtocolViolation(_))),
        "{result:?}"
    );

    // A corrupted deflate stream and a corrupted CRC.
    let mut bad = file.clone();
    let at = bad
        .windows(26)
        .position(|w| w == b"Metadata/slice_info.config")
        .unwrap()
        + 26;
    bad[at + 3] ^= 0xFF;
    let result = read_plate_filaments(&bad[..], 1).await;
    assert!(
        matches!(result, Err(Error::ProtocolViolation(_))),
        "{result:?}"
    );
    let stored = three_mf(ZIP_METHOD_STORED, b"");
    let mut bad = stored.clone();
    let at = bad.windows(5).position(|w| w == b"<?xml").unwrap();
    bad[at + 2] ^= 1;
    let result = read_plate_filaments(&bad[..], 1).await;
    assert!(
        matches!(result, Err(Error::ProtocolViolation(_))),
        "{result:?}"
    );
}

#[tokio::test]
async fn test_read_plate_filaments_refuses_zip64_and_oversized_members() {
    let mut file = three_mf(ZIP_METHOD_DEFLATE, b"");
    // Zip64 marker in the end record's central directory offset (bytes 16..20 of the record).
    let eocd = file.len() - ZIP_EOCD_LEN;
    file[eocd + 16..eocd + 20].copy_from_slice(&[0xFF; 4]);
    let result = read_plate_filaments(&file[..], 1).await;
    match result {
        Err(Error::ProtocolViolation(msg)) => assert!(msg.contains("Zip64"), "{msg}"),
        other => panic!("{other:?}"),
    }

    // A member claiming more than the cap is refused before it is read.
    let mut big = three_mf(ZIP_METHOD_STORED, b"");
    let name = b"Metadata/slice_info.config";
    let entry = big[..]
        .windows(46 + name.len())
        .position(|w| w.starts_with(b"PK\x01\x02") && &w[46..] == name)
        .unwrap();
    big[entry + 24..entry + 28].copy_from_slice(&(SLICE_INFO_MAX_BYTES as u32 + 1).to_le_bytes());
    let result = read_plate_filaments(&big[..], 1).await;
    match result {
        Err(Error::ProtocolViolation(msg)) => assert!(msg.contains("cap"), "{msg}"),
        other => panic!("{other:?}"),
    }
}
