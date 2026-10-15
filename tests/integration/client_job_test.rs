//! # Running job's plate: the source chain and `read_job_images`
//!
//! Frames are shaped on a P1S capture (2026-10-15) of plate 2 of a two-plate project sent from
//! Bambu Studio: a `project_file` echo with `param: "Metadata/plate_2.gcode"`, then status
//! pushes naming the file in `gcode_file` and never carrying `plate_idx`.

use bambino::client::{DummyTimer, JobPlate, PlateSource};
use bambino::ftps::{FtpsClient, TlsVersionCheck};
use bambino::io::TokioIo;
use bambino::models::PrinterModel;

use crate::common::client::{SERIAL, connect_idle_client, test_identity, with_broker};
use crate::common::io::{DataContainer, DummyTlsConnector, MockDataStreamFactory};
use crate::common::mock_ftps;
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

/// Runs `job_path()` after `frames`, with an FTPS mock answering `SIZE expected_path` with `reply`.
///
/// `expected_path` `None` means `job_path()` must not touch FTPS at all.
async fn job_path_after(
    frames: Vec<Vec<u8>>,
    size: Option<(&'static str, &'static [u8])>,
) -> Option<String> {
    let client = client_after(frames).await;
    let (control, server_control) = tokio::io::duplex(8192);
    let server = size.map(|(path, reply)| {
        tokio::spawn(mock_ftps::run_mock_server_size(server_control, path, reply))
    });
    let mut client = match server {
        Some(_) => {
            let ftps = FtpsClient::connect(
                TokioIo::new(control),
                DummyTlsConnector,
                MockDataStreamFactory::empty(),
                test_identity(PrinterModel::P1S),
                DummyTimer,
                TlsVersionCheck::Enforce,
            )
            .await
            .expect("FTPS handshake");
            client.with_attached_ftps(ftps)
        }
        // Never connected: a `job_path()` that dialed FTPS would fail instead of returning.
        None => client.with_ftps(
            DummyTlsConnector,
            MockDataStreamFactory::empty(),
            DummyTimer,
        ),
    };
    let path = client.job_path().await.expect("job_path");
    if let Some(server) = server {
        server.await.expect("FTPS mock panicked");
    }
    path
}

#[tokio::test]
async fn test_job_path_from_the_start_command_url() {
    let path = job_path_after(
        vec![start_echo(), status("RUNNING", FILE)],
        Some(("/632-multiplate-test_plate_2.gcode.3mf", b"213 75230\r\n")),
    )
    .await;
    assert_eq!(
        path.as_deref(),
        Some("/632-multiplate-test_plate_2.gcode.3mf")
    );
}

#[tokio::test]
async fn test_job_path_from_a_bare_gcode_file_at_the_root() {
    // A screen start: no echo, and `gcode_file` is the bare name.
    let path = job_path_after(
        vec![status("RUNNING", "multi2.gcode.3mf")],
        Some(("/multi2.gcode.3mf", b"213 75230\r\n")),
    )
    .await;
    assert_eq!(path.as_deref(), Some("/multi2.gcode.3mf"));
}

#[tokio::test]
async fn test_job_path_is_none_for_a_file_not_at_the_root() {
    // The screen-started file was in a folder: `/` + its bare name isn't there.
    let path = job_path_after(
        vec![status("RUNNING", "multi2.gcode.3mf")],
        Some(("/multi2.gcode.3mf", b"550 Could not get file size.\r\n")),
    )
    .await;
    assert_eq!(path, None);
}

#[tokio::test]
async fn test_job_path_is_none_without_a_card_file_name() {
    // X1 reports a path on internal storage; idle reports nothing.
    for gcode_file in ["/data/Metadata/plate_1.gcode", ""] {
        let path = job_path_after(vec![status("RUNNING", gcode_file)], None).await;
        assert_eq!(path, None, "{gcode_file:?}");
    }
}

/// A sent two-plate file: both plates' images, G-code for `sliced` only.
fn two_plate_file(sliced: &[u32]) -> Vec<u8> {
    let mut file = Vec::new();
    mock_ftps::zip_entry(&mut file, "[Content_Types].xml", 8, b"\x8d\x8eK");
    for plate in [1, 2] {
        let image = mock_ftps::test_png(format!("plate {plate}").as_bytes());
        mock_ftps::stored_entry(&mut file, &format!("Metadata/plate_{plate}.png"), &image);
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

/// Reads `file` through `read_job_images` on a client that knows no plate from telemetry.
async fn read_images(file: Vec<u8>) -> (Option<JobPlate>, Option<Vec<u8>>, Option<Vec<u8>>) {
    let (control, server_control) = tokio::io::duplex(8192);
    let container = DataContainer::default();
    let server = tokio::spawn(mock_ftps::run_mock_server_front_members(
        server_control,
        container.clone(),
        file,
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
    let (client, broker) = connect_idle_client(PrinterModel::P1S).await;
    let mut client = client.with_attached_ftps(ftps);

    let mut buf = vec![0u8; 64 * 1024];
    let images = client
        .read_job_images("/model/job.3mf", &mut buf)
        .await
        .expect("read");
    let result = (
        images.plate,
        images.plate_image.map(<[u8]>::to_vec),
        images.pick_image.map(<[u8]>::to_vec),
    );
    server.await.expect("FTPS mock panicked");
    drop(broker);
    result
}

#[tokio::test]
async fn test_read_job_images_takes_the_plate_from_the_file() {
    let (plate, plate_image, pick_image) = read_images(two_plate_file(&[2])).await;
    assert_eq!(
        plate,
        Some(JobPlate {
            plate: 2,
            source: PlateSource::ProjectFile,
        })
    );
    assert_eq!(plate_image, Some(mock_ftps::test_png(b"plate 2")));
    assert_eq!(pick_image, Some(mock_ftps::test_png(b"pick 2")));
}

#[tokio::test]
async fn test_read_job_images_refuses_to_guess() {
    // G-code for both plates: the file can't say which one is printing.
    assert_eq!(
        read_images(two_plate_file(&[1, 2])).await,
        (None, None, None)
    );
}
