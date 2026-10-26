//! # Running job's plate and file, and plate images: the source chain, `job_file`, `read_plate_images` and `read_running_job_images`
//!
//! Frames are shaped on a P1S capture (2026-10-15) of plate 2 of a two-plate project sent from
//! Bambu Studio: a `project_file` echo with `param: "Metadata/plate_2.gcode"`, then status
//! pushes naming the file in `gcode_file` and never carrying `plate_idx`.

use bambino::client::{
    DummyTimer, FileKind, FileTime, JobPlate, PlateSource, PrinterFile, StorageKind,
};
use bambino::ftps::{FtpTimestamp, FtpsClient, TlsVersionCheck};
use bambino::io::TokioIo;
use bambino::models::PrinterModel;
use tokio::io::DuplexStream;
use tokio::task::JoinHandle;

use crate::common::client::{SERIAL, test_identity, with_broker};
use crate::common::io::{DataContainer, DummyTlsConnector, MockDataStreamFactory};
use crate::common::mock_ftps::{self, Step};
use crate::common::mock_mqtt::{ReportPublisher, handle_mqtt_handshake};

const FILE: &str = "632-multiplate-test_plate_2.gcode.3mf";

/// The printer's echo of a start command for plate 2 of `FILE`.
fn start_echo() -> Vec<u8> {
    format!(
        r#"{{"print":{{"command":"project_file","param":"Metadata/plate_2.gcode","file":"{FILE}","url":"ftp://{FILE}","subtask_name":"632-multiplate-test_plate_2","sequence_id":"20003","reason":"success","result":"success"}}}}"#
    )
    .into_bytes()
}

fn status(gcode_state: &str, gcode_file: &str) -> Vec<u8> {
    format!(
        r#"{{"print":{{"command":"push_status","gcode_state":"{gcode_state}","gcode_file":"{gcode_file}","sequence_id":"1"}}}}"#
    )
    .into_bytes()
}

/// Connects a client whose broker publishes `frames`, and polls each one.
async fn client_after(
    frames: Vec<Vec<u8>>,
) -> crate::common::client::TestClient<TokioIo<tokio::io::DuplexStream>> {
    let count = frames.len();
    let (mut client, task) = with_broker(SERIAL, PrinterModel::P1S, |mut stream| async move {
        let mut reports = ReportPublisher::new(SERIAL);
        handle_mqtt_handshake(&mut stream).await;
        for frame in frames {
            reports.publish(&mut stream, &frame).await;
        }
        stream
    })
    .await;
    for _ in 0..count {
        client.poll_telemetry().await.expect("frame");
    }
    drop(task);
    client
}

#[tokio::test]
async fn test_job_plate_from_the_start_command_echo() {
    let client = client_after(vec![
        start_echo(),
        status("IDLE", FILE),
        status("RUNNING", FILE),
    ])
    .await;
    assert_eq!(
        client.job_plate(),
        Some(JobPlate {
            plate: 2,
            source: PlateSource::StartCommand,
        })
    );
}

#[tokio::test]
async fn test_job_plate_drops_a_start_for_another_file_or_an_ended_job() {
    // The running file isn't the one the echo started.
    let client = client_after(vec![start_echo(), status("RUNNING", "other.gcode.3mf")]).await;
    assert_eq!(client.job_plate(), None);

    // The job the echo started has ended: a later job of the same file, started without an echo
    // this client saw, may be another plate.
    let client = client_after(vec![
        start_echo(),
        status("RUNNING", FILE),
        status("FINISH", FILE),
        status("RUNNING", FILE),
    ])
    .await;
    assert_eq!(client.job_plate(), None);
}

#[tokio::test]
async fn test_job_plate_from_plate_idx_until_the_file_changes() {
    let with_plate_idx = format!(
        r#"{{"print":{{"command":"push_status","gcode_state":"RUNNING","gcode_file":"{FILE}","plate_idx":3,"sequence_id":"1"}}}}"#
    )
    .into_bytes();
    let client = client_after(vec![with_plate_idx.clone()]).await;
    assert_eq!(
        client.job_plate(),
        Some(JobPlate {
            plate: 3,
            source: PlateSource::Telemetry,
        })
    );
    // Telemetry outranks the echo.
    let client = client_after(vec![start_echo(), with_plate_idx.clone()]).await;
    assert_eq!(
        client.job_plate().map(|p| p.source),
        Some(PlateSource::Telemetry)
    );
    // A new file without `plate_idx`: the old value doesn't describe it.
    let client = client_after(vec![with_plate_idx, status("RUNNING", "next.gcode.3mf")]).await;
    assert_eq!(client.job_plate(), None);
}

type MockFtps =
    FtpsClient<TokioIo<DuplexStream>, DummyTlsConnector, MockDataStreamFactory, DummyTimer>;

/// A connected FTPS client whose server runs `steps` after the handshake.
async fn ftps_session(steps: Vec<Step>) -> (MockFtps, JoinHandle<()>) {
    let (control, server_control) = tokio::io::duplex(8192);
    let container = DataContainer::default();
    let server = tokio::spawn(mock_ftps::run_script(
        server_control,
        container.clone(),
        steps,
    ));
    let ftps = FtpsClient::connect(
        TokioIo::new(control),
        DummyTlsConnector,
        MockDataStreamFactory::new(container),
        test_identity(PrinterModel::P1S),
        DummyTimer,
        TlsVersionCheck::Enforce,
    )
    .await
    .expect("FTPS handshake");
    (ftps, server)
}

/// Runs `job_file()` after `frames`, against an FTPS session running `steps`.
async fn job_file_after(frames: Vec<Vec<u8>>, steps: Vec<Step>) -> Option<PrinterFile> {
    let (ftps, server) = ftps_session(steps).await;
    let mut client = client_after(frames).await.with_attached_ftps(ftps);
    let file = client.job_file().await.expect("job_file");
    server.await.expect("FTPS mock panicked");
    file
}

const MDTM_AT: FtpTimestamp = FtpTimestamp {
    year: 2025,
    month: 10,
    day: 16,
    hour: 9,
    minute: 30,
    second: 15,
};

#[tokio::test]
async fn test_job_file_from_the_start_command_url() {
    let path = "/632-multiplate-test_plate_2.gcode.3mf";
    let file = job_file_after(
        vec![start_echo(), status("RUNNING", FILE)],
        vec![
            Step::expect(&format!("SIZE {path}"), "213 75230"),
            Step::expect(&format!("MDTM {path}"), "213 20251016093015"),
        ],
    )
    .await
    .expect("file");
    assert_eq!(file.path(), path);
    assert_eq!(file.name, FILE);
    assert_eq!(file.size, 75230);
    assert_eq!(file.kind, FileKind::Model);
    assert_eq!(file.storage, StorageKind::External);
    assert_eq!(
        file.modified,
        FileTime::Calendar {
            at: MDTM_AT,
            year_inferred: false,
        }
    );
}

#[tokio::test]
async fn test_job_file_from_a_bare_gcode_file_at_the_root_without_mdtm() {
    // A screen start: no echo, and `gcode_file` is the bare name. This firmware has no `MDTM`.
    let file = job_file_after(
        vec![status("RUNNING", "multi2.gcode.3mf")],
        vec![
            Step::expect("SIZE /multi2.gcode.3mf", "213 75230"),
            Step::expect("MDTM /multi2.gcode.3mf", "502 Command not implemented."),
        ],
    )
    .await
    .expect("file");
    assert_eq!(file.path(), "/multi2.gcode.3mf");
    assert_eq!(file.modified, FileTime::Unknown);
}

#[tokio::test]
async fn test_job_file_is_none_for_a_file_not_at_the_root() {
    // The screen-started file was in a folder: `/` + its bare name isn't there.
    let file = job_file_after(
        vec![status("RUNNING", "multi2.gcode.3mf")],
        vec![Step::expect(
            "SIZE /multi2.gcode.3mf",
            "550 Could not get file size.",
        )],
    )
    .await;
    assert_eq!(file, None);
}

#[tokio::test]
async fn test_job_file_is_none_without_a_card_file_name() {
    // X1 reports a path on internal storage; idle reports nothing.
    for gcode_file in ["/data/Metadata/plate_1.gcode", ""] {
        // Never connected: a `job_file()` that dialed FTPS would fail instead of returning.
        let mut client = client_after(vec![status("RUNNING", gcode_file)])
            .await
            .with_ftps(
                DummyTlsConnector,
                MockDataStreamFactory::empty(),
                DummyTimer,
            );
        let file = client.job_file().await.expect("job_file");
        assert_eq!(file, None, "{gcode_file:?}");
    }
}

/// A sent two-plate file: both plates' images, G-code for `sliced` only.
fn two_plate_file(sliced: &[u32]) -> Vec<u8> {
    let mut file = Vec::new();
    mock_ftps::zip_entry(&mut file, "[Content_Types].xml", 8, b"\x8d\x8eK");
    for plate in [1, 2] {
        let image = mock_ftps::test_png(format!("plate {plate}").as_bytes());
        mock_ftps::stored_entry(&mut file, &format!("Metadata/plate_{plate}.png"), &image);
        let small = mock_ftps::test_png(format!("small {plate}").as_bytes());
        mock_ftps::stored_entry(
            &mut file,
            &format!("Metadata/plate_{plate}_small.png"),
            &small,
        );
    }
    for plate in [1, 2] {
        let image = mock_ftps::test_png(format!("pick {plate}").as_bytes());
        mock_ftps::stored_entry(&mut file, &format!("Metadata/pick_{plate}.png"), &image);
    }
    mock_ftps::zip_entry(&mut file, "3D/3dmodel.model", 8, b"model");
    mock_ftps::zip_entry(
        &mut file,
        "Metadata/project_settings.config",
        8,
        b"settings",
    );
    for plate in sliced {
        let name = format!("Metadata/plate_{plate}.gcode.md5");
        mock_ftps::zip_entry(&mut file, &name, 8, b"md5");
    }
    for plate in sliced {
        let name = format!("Metadata/plate_{plate}.gcode");
        mock_ftps::zip_entry(&mut file, &name, 8, &[b';'; 64 * 1024]);
    }
    file
}

/// Plate `plate`'s image, thumbnail and pick image as [`two_plate_file`] writes them.
fn images_of(plate: u32) -> [Option<Vec<u8>>; 3] {
    ["plate", "small", "pick"]
        .map(|kind| Some(mock_ftps::test_png(format!("{kind} {plate}").as_bytes())))
}

/// Lists `/other.3mf` at the card root, then reads `file` as its contents with `read_plate_images(plate)`, after `frames`.
async fn read_listed(
    frames: Vec<Vec<u8>>,
    file: Vec<u8>,
    plate: Option<u32>,
) -> (Option<u32>, bool, [Option<Vec<u8>>; 3]) {
    let (ftps, server) = ftps_session(vec![
        Step::List("/cache".into(), Vec::new()),
        Step::List(
            "/".into(),
            b"-rw-r--r--    1 1000     1000       75230 Jun 17  2025 other.3mf\r\n".to_vec(),
        ),
        Step::RetrStopped("/other.3mf".into(), file),
    ])
    .await;
    let mut client = client_after(frames).await.with_attached_ftps(ftps);
    let files = client.list_files(FileKind::Model).await.expect("list");
    let [listed] = files.as_slice() else {
        panic!("expected one file, got {files:?}");
    };

    let mut buf = vec![0u8; 64 * 1024];
    let images = client
        .read_plate_images(listed, plate, &mut buf)
        .await
        .expect("read");
    let result = (
        images.plate,
        images.from_file,
        [images.plate_image, images.small_image, images.pick_image]
            .map(|image| image.map(<[u8]>::to_vec)),
    );
    server.await.expect("FTPS mock panicked");
    result
}

/// What [`read_listed_lists`] asks of the listed file.
#[derive(Clone, Copy)]
enum Ask {
    /// `read_gcode_plates`.
    Plates,
    /// `read_plate_images(plate)`.
    Images(Option<u32>),
}

/// Lists `/other.3mf` holding `file`, asks `ask`, and returns the plate list it carried, with
/// `read_plate_images`' plate and the first image's presence.
async fn read_listed_lists(file: Vec<u8>, ask: Ask) -> (Option<Vec<u32>>, Option<u32>, bool) {
    let (ftps, server) = ftps_session(vec![
        Step::List("/cache".into(), Vec::new()),
        Step::List(
            "/".into(),
            b"-rw-r--r--    1 1000     1000       75230 Jun 17  2025 other.3mf\r\n".to_vec(),
        ),
        Step::RetrStopped("/other.3mf".into(), file),
    ])
    .await;
    let mut client = client_after(Vec::new()).await.with_attached_ftps(ftps);
    let files = client.list_files(FileKind::Model).await.expect("list");
    let [listed] = files.as_slice() else {
        panic!("expected one file, got {files:?}");
    };
    let mut buf = vec![0u8; 64 * 1024];
    let result = match ask {
        Ask::Plates => {
            let plates = client
                .read_gcode_plates(listed, &mut buf)
                .await
                .expect("read");
            (Some(plates), None, false)
        }
        Ask::Images(plate) => {
            let images = client
                .read_plate_images(listed, plate, &mut buf)
                .await
                .expect("read");
            (
                images.gcode_plates,
                images.plate,
                images.plate_image.is_some(),
            )
        }
    };
    server.await.expect("FTPS mock panicked");
    result
}

#[tokio::test]
async fn test_read_gcode_plates_lists_every_sliced_plate() {
    let (plates, _, _) = read_listed_lists(two_plate_file(&[1, 2]), Ask::Plates).await;
    assert_eq!(plates, Some(vec![1, 2]));
}

#[tokio::test]
async fn test_read_plate_images_carries_the_plate_list() {
    // One plate: its images and the list.
    assert_eq!(
        read_listed_lists(two_plate_file(&[2]), Ask::Images(None)).await,
        (Some(vec![2]), Some(2), true)
    );
    // Two plates: no plate, but the list to offer.
    assert_eq!(
        read_listed_lists(two_plate_file(&[1, 2]), Ask::Images(None)).await,
        (Some(vec![1, 2]), None, false)
    );
    // A plate given: the list isn't read.
    assert_eq!(
        read_listed_lists(two_plate_file(&[1, 2]), Ask::Images(Some(1))).await,
        (None, Some(1), true)
    );
}

#[tokio::test]
async fn test_read_plate_images_takes_the_plate_from_the_file() {
    let (plate, from_file, images) = read_listed(Vec::new(), two_plate_file(&[2]), None).await;
    assert_eq!(plate, Some(2));
    assert!(from_file);
    assert_eq!(images, images_of(2));
}

#[tokio::test]
async fn test_read_plate_images_refuses_to_guess() {
    // G-code for both plates: the file can't say which one to show.
    assert_eq!(
        read_listed(Vec::new(), two_plate_file(&[1, 2]), None).await,
        (None, false, [None, None, None])
    );
}

#[tokio::test]
async fn test_read_plate_images_reads_the_plate_given() {
    let (plate, from_file, images) =
        read_listed(Vec::new(), two_plate_file(&[1, 2]), Some(1)).await;
    assert_eq!(plate, Some(1));
    assert!(!from_file);
    assert_eq!(images, images_of(1));
}

#[tokio::test]
async fn test_read_plate_images_ignores_the_running_jobs_plate() {
    // Plate 2 of another file is printing; this file's own G-code says plate 1.
    let running = format!(
        r#"{{"print":{{"command":"push_status","gcode_state":"RUNNING","gcode_file":"{FILE}","plate_idx":2,"sequence_id":"1"}}}}"#
    )
    .into_bytes();
    let (plate, from_file, images) = read_listed(vec![running], two_plate_file(&[1]), None).await;
    assert_eq!(plate, Some(1));
    assert!(from_file);
    assert_eq!(images, images_of(1));
}

/// Runs `read_running_job_images` over FTPS after `frames`, finding the job's file at `path`, which holds `file`.
async fn read_running(
    frames: Vec<Vec<u8>>,
    path: &str,
    file: Vec<u8>,
) -> (Option<JobPlate>, Option<PrinterFile>, [Option<Vec<u8>>; 3]) {
    let (ftps, server) = ftps_session(vec![
        Step::expect(&format!("SIZE {path}"), "213 75230"),
        Step::expect(&format!("MDTM {path}"), "213 20251016093015"),
        Step::RetrStopped(path.into(), file),
    ])
    .await;
    let mut client = client_after(frames).await.with_attached_ftps(ftps);
    let mut buf = vec![0u8; 64 * 1024];
    let images = client
        .read_running_job_images(&mut buf)
        .await
        .expect("read");
    let result = (
        images.plate,
        images.file,
        [images.plate_image, images.small_image, images.pick_image]
            .map(|image| image.map(<[u8]>::to_vec)),
    );
    server.await.expect("FTPS mock panicked");
    result
}

#[tokio::test]
async fn test_read_running_job_images_takes_the_plate_from_the_job_file() {
    // A screen start: no plate from telemetry, so the file's G-code names it.
    let (plate, file, images) = read_running(
        vec![status("RUNNING", "multi2.gcode.3mf")],
        "/multi2.gcode.3mf",
        two_plate_file(&[2]),
    )
    .await;
    assert_eq!(
        plate,
        Some(JobPlate {
            plate: 2,
            source: PlateSource::ProjectFile,
        })
    );
    assert_eq!(
        file.as_ref().map(PrinterFile::path),
        Some("/multi2.gcode.3mf")
    );
    assert_eq!(images, images_of(2));
}

#[tokio::test]
async fn test_read_running_job_images_keeps_the_start_commands_plate() {
    // Both plates sliced: only the echoed start command says plate 2.
    let (plate, file, images) = read_running(
        vec![start_echo(), status("RUNNING", FILE)],
        "/632-multiplate-test_plate_2.gcode.3mf",
        two_plate_file(&[1, 2]),
    )
    .await;
    assert_eq!(
        plate,
        Some(JobPlate {
            plate: 2,
            source: PlateSource::StartCommand,
        })
    );
    assert_eq!(file.map(|f| f.name).as_deref(), Some(FILE));
    assert_eq!(images, images_of(2));
}
