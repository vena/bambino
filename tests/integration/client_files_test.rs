//! # `PrinterClient::list_files`, `list_files_each`, `download_file` and `delete_file` over FTPS
//!
//! Each test dials FTPS lazily through one [`MockDataStreamFactory`], whose slot serves the
//! control stream first and then each passive data stream the script hands it.

use std::sync::atomic::Ordering;

use bambino::client::{
    DummyFactory, DummyRawIo, DummyTimer, DummyTls, FileKind, FileTime, PrinterClient, PrinterFile,
    StorageKind,
};
use bambino::error::Error;
use bambino::ftps::FtpTimestamp;
use bambino::io::TokioIo;
use bambino::models::PrinterModel;
use tokio::io::DuplexStream;
use tokio::task::JoinHandle;

use crate::common::client::test_identity;
use crate::common::io::{DataContainer, DummyTlsConnector, MockDataStreamFactory};
use crate::common::mock_ftps::{self, Step};

type FilesClient = PrinterClient<
    DummyRawIo,
    DummyTls,
    DummyFactory,
    DummyTimer,
    TokioIo<DuplexStream>,
    DummyTlsConnector,
    MockDataStreamFactory,
    DummyTimer,
>;

/// A P1S client with FTPS configured, plus the factory's slot and its abort count.
struct Fixture {
    client: FilesClient,
    container: DataContainer,
    aborted: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl Fixture {
    fn new() -> Self {
        let factory = MockDataStreamFactory::empty();
        let container = factory.active_stream.clone();
        let aborted = factory.aborted.clone();
        let client = PrinterClient::new(DummyTls, DummyFactory, test_identity(PrinterModel::P1S))
            .with_ftps(DummyTlsConnector, factory, DummyTimer);
        Self {
            client,
            container,
            aborted,
        }
    }

    /// Queues one FTPS session running `steps`, for the client's next dial.
    async fn session(&self, steps: Vec<Step>) -> JoinHandle<()> {
        let (client_control, server_control) = tokio::io::duplex(8192);
        *self.container.lock().await = Some(TokioIo::new(client_control));
        tokio::spawn(mock_ftps::run_script(
            server_control,
            self.container.clone(),
            steps,
        ))
    }
}

fn list(path: &str, lines: &str) -> Step {
    Step::List(path.to_string(), lines.as_bytes().to_vec())
}

const ROOT: &str = "drwxr-xr-x    2 1000     1000         4096 Jun 17  2025 cache\r\n\
-rw-r--r--    1 1000     1000       75230 Jun 17  2025 benchy.gcode.3mf\r\n\
-rw-r--r--    1 1000     1000         120 Jun 17  2025 notes.gcode\r\n\
-rw-r--r--    1 1000     1000          99 Jun 18  2025 UPPER.3MF\r\n";

async fn root_models(fixture: &mut Fixture) -> Vec<PrinterFile> {
    fixture
        .client
        .list_files(FileKind::Model)
        .await
        .expect("list_files")
}

#[tokio::test]
async fn test_list_models_keeps_only_3mf_files_at_the_root() {
    let mut fx = Fixture::new();
    let server = fx.session(vec![list("/cache", ""), list("/", ROOT)]).await;
    let files = root_models(&mut fx).await;
    let names: Vec<_> = files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["benchy.gcode.3mf", "UPPER.3MF"]);
    let file = &files[0];
    assert_eq!(file.size, 75230);
    assert_eq!(file.kind, FileKind::Model);
    assert_eq!(file.storage, StorageKind::External);
    assert_eq!(
        file.modified,
        FileTime::Calendar {
            at: FtpTimestamp {
                year: 2025,
                month: 6,
                day: 17,
                hour: 0,
                minute: 0,
                second: 0,
            },
            year_inferred: false,
        }
    );
    server.await.expect("FTPS mock panicked");
}

#[tokio::test]
async fn test_list_models_lists_cache_then_root_and_keeps_each_path() {
    let mut fx = Fixture::new();
    let server = fx
        .session(vec![list("/cache", CACHE), list("/", ROOT)])
        .await;
    let files = root_models(&mut fx).await;
    let paths: Vec<_> = files.iter().map(PrinterFile::path).collect();
    assert_eq!(
        paths,
        ["/cache/benchy.gcode.3mf", "/benchy.gcode.3mf", "/UPPER.3MF"]
    );
    // Same name, different files.
    assert_eq!(files[0].name, files[1].name);
    assert_ne!(files[0].size, files[1].size);
    server.await.expect("FTPS mock panicked");
}

const TIMELAPSES: &str = "drwxr-xr-x    2 1000     1000         4096 Nov 14  2025 thumbnail\r\n\
-rw-r--r--    1 1000     1000     2600000 Dec 15  2025 video_2025-12-15_11-47-18.avi\r\n";

#[tokio::test]
async fn test_read_thumbnail_reads_the_matching_jpg() {
    let mut fx = Fixture::new();
    let cover = b"\xff\xd8 jpeg \xff\xd9".to_vec();
    let server = fx
        .session(vec![
            list("/timelapse", TIMELAPSES),
            Step::Retr(
                "/timelapse/thumbnail/video_2025-12-15_11-47-18.jpg".into(),
                cover.clone(),
            ),
            Step::expect(
                "SIZE /timelapse/thumbnail/video_2025-12-15_11-47-18.jpg",
                &format!("213 {}", cover.len()),
            ),
            Step::RetrMissing("/timelapse/thumbnail/video_2025-12-15_11-47-18.jpg".into()),
        ])
        .await;
    let video = fx
        .client
        .list_files(FileKind::Timelapse)
        .await
        .expect("list")
        .remove(0);

    let mut got = Vec::new();
    let n = fx
        .client
        .read_thumbnail(&video, |chunk| {
            got.extend_from_slice(chunk);
            Ok(())
        })
        .await
        .expect("thumbnail");
    assert_eq!(n, Some(cover.len() as u64));
    assert_eq!(got, cover);

    // No cover: `None`, and the session stays usable.
    assert_eq!(
        fx.client
            .read_thumbnail(&video, |_| Ok(()))
            .await
            .expect("read"),
        None
    );
    assert!(fx.client.is_ftps_connected());
    server.await.expect("FTPS mock panicked");
}

#[tokio::test]
async fn test_read_thumbnail_550_after_the_cover_arrived_is_an_error() {
    // The `SIZE` check's 550 comes after `sink` saw the cover: not "no cover".
    let mut fx = Fixture::new();
    let server = fx
        .session(vec![
            list("/timelapse", TIMELAPSES),
            Step::Retr(
                "/timelapse/thumbnail/video_2025-12-15_11-47-18.jpg".into(),
                b"jpeg".to_vec(),
            ),
            Step::expect(
                "SIZE /timelapse/thumbnail/video_2025-12-15_11-47-18.jpg",
                "550 Could not get file size.",
            ),
        ])
        .await;
    let video = fx
        .client
        .list_files(FileKind::Timelapse)
        .await
        .expect("list")
        .remove(0);
    assert!(matches!(
        fx.client.read_thumbnail(&video, |_| Ok(())).await,
        Err(Error::FtpReply { code: 550, .. })
    ));
    server.await.expect("FTPS mock panicked");
}

#[tokio::test]
async fn test_read_thumbnail_is_none_for_a_print_file_without_a_request() {
    let mut fx = Fixture::new();
    // The script ends after the listing: a RETR would find the session closed and fail.
    let server = fx.session(vec![list("/cache", ""), list("/", ROOT)]).await;
    let file = root_models(&mut fx).await.remove(0);
    assert_eq!(
        fx.client
            .read_thumbnail(&file, |_| Ok(()))
            .await
            .expect("read"),
        None
    );
    server.await.expect("FTPS mock panicked");
}

#[tokio::test]
async fn test_read_plate_images_is_none_for_a_timelapse_without_a_request() {
    let mut fx = Fixture::new();
    // The script ends after the listing: a RETR would find the session closed and fail.
    let server = fx.session(vec![list("/timelapse", TIMELAPSES)]).await;
    let video = fx
        .client
        .list_files(FileKind::Timelapse)
        .await
        .expect("list")
        .remove(0);
    let mut buf = [0u8; 64];
    let images = fx
        .client
        .read_plate_images(&video, Some(1), &mut buf)
        .await
        .expect("read");
    assert_eq!(images.plate, None);
    assert!(!images.from_file);
    assert_eq!(
        [images.plate_image, images.small_image, images.pick_image],
        [None, None, None]
    );
    server.await.expect("FTPS mock panicked");
}

#[tokio::test]
async fn test_list_timelapses_skips_the_thumbnail_directory() {
    let mut fx = Fixture::new();
    let server = fx
        .session(vec![list(
            "/timelapse",
            "drwxr-xr-x    2 1000     1000         4096 Jun 17  2025 thumbnail\r\n\
             -rw-r--r--    1 1000     1000     1632221 Jun 17  2025 video_2025-06-17_12-12-18.mp4\r\n",
        )])
        .await;
    let files = fx
        .client
        .list_files(FileKind::Timelapse)
        .await
        .expect("list_files");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name, "video_2025-06-17_12-12-18.mp4");
    assert_eq!(files[0].kind, FileKind::Timelapse);
    server.await.expect("FTPS mock panicked");
}

#[tokio::test]
async fn test_list_videos_is_empty_without_touching_ftps() {
    // No session queued: a dial would be refused.
    let mut fx = Fixture::new();
    let files = fx.client.list_files(FileKind::Video).await.expect("list");
    assert!(files.is_empty());
    let mut delivered = 0;
    fx.client
        .list_files_each(FileKind::Video, |_| delivered += 1)
        .await
        .expect("list_files_each");
    assert_eq!(delivered, 0);
}

const CACHE: &str = "-rw-r--r--    1 1000     1000        1000 Nov 14  2025 benchy.gcode.3mf\r\n\
-rw-r--r--    1 1000     1000         421 Nov 14  2025 1_benchy.bbl\r\n";

#[tokio::test]
async fn test_list_files_each_delivers_what_list_files_returns() {
    let mut fx = Fixture::new();
    let server = fx
        .session(vec![
            list("/cache", CACHE),
            list("/", ROOT),
            list("/cache", CACHE),
            list("/", ROOT),
            list("/timelapse", TIMELAPSES),
            list("/timelapse", TIMELAPSES),
        ])
        .await;
    for kind in [FileKind::Model, FileKind::Timelapse] {
        let listed = fx.client.list_files(kind).await.expect("list_files");
        let mut delivered = Vec::new();
        fx.client
            .list_files_each(kind, |file| delivered.push(file))
            .await
            .expect("list_files_each");
        assert!(!listed.is_empty());
        assert_eq!(delivered, listed, "{kind:?}");
    }
    server.await.expect("FTPS mock panicked");
}

#[tokio::test]
async fn test_list_files_each_delivers_the_first_directory_before_the_second_fails() {
    let mut fx = Fixture::new();
    // The session ends after `/cache`, so listing `/` fails.
    let server = fx.session(vec![list("/cache", CACHE)]).await;
    let mut delivered = Vec::new();
    let result = fx
        .client
        .list_files_each(FileKind::Model, |file| delivered.push(file))
        .await;
    assert!(result.is_err(), "{result:?}");
    let paths: Vec<_> = delivered.iter().map(PrinterFile::path).collect();
    assert_eq!(paths, ["/cache/benchy.gcode.3mf"]);
    server.await.expect("FTPS mock panicked");
}

#[tokio::test]
async fn test_download_streams_the_file_and_rechecks_size() {
    let mut fx = Fixture::new();
    let body = vec![7u8; 10_000];
    let server = fx
        .session(vec![
            list("/cache", ""),
            list("/", ROOT),
            Step::Retr("/benchy.gcode.3mf".into(), body.clone()),
            Step::expect("SIZE /benchy.gcode.3mf", "213 10000"),
            Step::Retr("/benchy.gcode.3mf".into(), body.clone()),
            Step::expect("SIZE /benchy.gcode.3mf", "213 10001"),
        ])
        .await;
    let file = root_models(&mut fx).await.remove(0);

    let mut got = Vec::new();
    let n = fx
        .client
        .download_file(&file, |chunk| {
            got.extend_from_slice(chunk);
            Ok(())
        })
        .await
        .expect("download");
    assert_eq!(n, 10_000);
    assert_eq!(got, body);

    // A SIZE that disagrees with the bytes read is a truncated transfer.
    assert!(matches!(
        fx.client.download_file(&file, |_| Ok(())).await,
        Err(Error::ProtocolViolation(_))
    ));
    server.await.expect("FTPS mock panicked");
}

#[tokio::test]
async fn test_sink_error_ends_the_session_and_the_next_call_reconnects() {
    let mut fx = Fixture::new();
    let server = fx
        .session(vec![
            list("/cache", ""),
            list("/", ROOT),
            Step::RetrStopped("/benchy.gcode.3mf".into(), vec![1u8; 2048]),
        ])
        .await;
    let file = root_models(&mut fx).await.remove(0);

    let result = fx
        .client
        .download_file(&file, |_| Err(Error::InvalidArgument("disk full".into())))
        .await;
    assert!(
        matches!(result, Err(Error::InvalidArgument(_))),
        "{result:?}"
    );
    assert_eq!(fx.aborted.load(Ordering::SeqCst), 1, "data channel reset");
    assert!(!fx.client.is_ftps_connected(), "session left poisoned");
    server.await.expect("FTPS mock panicked");

    let server = fx.session(vec![list("/cache", ""), list("/", ROOT)]).await;
    assert_eq!(root_models(&mut fx).await.len(), 2);
    server.await.expect("FTPS mock panicked");
}

#[tokio::test]
async fn test_delete_file_deletes_the_listed_path() {
    let mut fx = Fixture::new();
    let server = fx
        .session(vec![
            list("/cache", ""),
            list("/", ROOT),
            Step::expect("DELE /benchy.gcode.3mf", "250 Delete operation successful."),
        ])
        .await;
    let file = root_models(&mut fx).await.remove(0);
    fx.client.delete_file(&file).await.expect("delete");
    server.await.expect("FTPS mock panicked");
}

/// A root listing whose one model has no year: `Mar 01 10:00`.
const RECENT: &str = "-rw-r--r--    1 1000     1000          10 Mar 01 10:00 recent.3mf\r\n";

/// The probe steps, with `mdtm` as the `MDTM` reply.
fn probe(mdtm: &str) -> Vec<Step> {
    vec![
        Step::Stor("/bambino_clock_probe.txt".into(), 19),
        Step::expect("SIZE /bambino_clock_probe.txt", "213 19"),
        Step::expect("MDTM /bambino_clock_probe.txt", mdtm),
        Step::expect(
            "DELE /bambino_clock_probe.txt",
            "250 Delete operation successful.",
        ),
    ]
}

/// The modified time of the one model in a [`RECENT`] listing.
async fn recent_time(fx: &mut Fixture) -> FileTime {
    let files = root_models(fx).await;
    assert_eq!(files.len(), 1);
    files[0].modified
}

#[tokio::test]
async fn test_probe_is_off_by_default() {
    // The script has no STOR or MDTM: sending either fails the mock.
    let mut fx = Fixture::new();
    let server = fx
        .session(vec![list("/cache", ""), list("/", RECENT)])
        .await;
    let FileTime::Calendar { at, year_inferred } = recent_time(&mut fx).await else {
        panic!("host clock gives a year");
    };
    assert!(year_inferred);
    assert_eq!((at.month, at.day, at.hour, at.minute), (3, 1, 10, 0));
    server.await.expect("FTPS mock panicked");
}

#[tokio::test]
async fn test_probe_runs_once_per_session() {
    let mut fx = Fixture::new();
    fx.client = fx.client.with_printer_clock_probe(true);
    // The printer's clock reads 2026-02-02, so `Mar 01` is last year's.
    let mut steps = probe("213 20260202085730");
    steps.extend([
        list("/cache", ""),
        list("/", RECENT),
        list("/cache", ""),
        list("/", RECENT),
        Step::expect("QUIT", "221 Goodbye."),
    ]);
    let server = fx.session(steps).await;
    let expected = FileTime::Calendar {
        at: FtpTimestamp {
            year: 2025,
            month: 3,
            day: 1,
            hour: 10,
            minute: 0,
            second: 0,
        },
        year_inferred: true,
    };
    assert_eq!(recent_time(&mut fx).await, expected);
    // The second listing reuses the reading.
    assert_eq!(recent_time(&mut fx).await, expected);
    fx.client.disconnect_ftps().await;
    server.await.expect("FTPS mock panicked");

    // A new session probes again: this time the printer's clock is past `Mar 01`.
    let mut steps = probe("213 20260402085730");
    steps.extend([list("/cache", ""), list("/", RECENT)]);
    let server = fx.session(steps).await;
    let FileTime::Calendar { at, .. } = recent_time(&mut fx).await else {
        panic!("probed clock gives a year");
    };
    assert_eq!(at.year, 2026);
    server.await.expect("FTPS mock panicked");
}

#[tokio::test]
async fn test_probe_without_mdtm_falls_back_once() {
    let mut fx = Fixture::new();
    fx.client = fx.client.with_printer_clock_probe(true);
    let mut steps = probe("502 Command not implemented.");
    // The second listing doesn't retry the probe.
    steps.extend([
        list("/cache", ""),
        list("/", RECENT),
        list("/cache", ""),
        list("/", RECENT),
    ]);
    let server = fx.session(steps).await;
    for _ in 0..2 {
        assert!(matches!(
            recent_time(&mut fx).await,
            FileTime::Calendar {
                year_inferred: true,
                ..
            }
        ));
    }
    assert_eq!(fx.client.printer_clock_reading(), None);
    server.await.expect("FTPS mock panicked");
}

/// The year a [`RECENT`] listing's model gets.
async fn recent_year(fx: &mut Fixture) -> i32 {
    let FileTime::Calendar { at, .. } = recent_time(fx).await else {
        panic!("a clock gives a year");
    };
    at.year
}

#[tokio::test]
async fn test_kept_reading_lists_later_sessions_without_probing() {
    let mut fx = Fixture::new();
    fx.client = fx.client.with_printer_clock_probe(true);
    assert_eq!(fx.client.printer_clock_reading(), None);
    // The printer's clock reads 2026-02-02, so `Mar 01` is last year's.
    let mut steps = probe("213 20260202085730");
    steps.extend([
        list("/cache", ""),
        list("/", RECENT),
        Step::expect("QUIT", "221 Goodbye."),
    ]);
    let server = fx.session(steps).await;
    assert_eq!(recent_year(&mut fx).await, 2025);
    let reading = fx
        .client
        .printer_clock_reading()
        .expect("probe read the clock");
    assert_eq!(
        reading.at,
        FtpTimestamp {
            year: 2026,
            month: 2,
            day: 2,
            hour: 8,
            minute: 57,
            second: 30,
        }
    );
    fx.client.disconnect_ftps().await;
    server.await.expect("FTPS mock panicked");
    assert_eq!(fx.client.printer_clock_reading(), None);

    // Supplied back, the reading lists a new session the same way, with the probe still on and
    // no STOR or MDTM in the script.
    fx.client.set_printer_clock_reading(Some(reading));
    let server = fx
        .session(vec![list("/cache", ""), list("/", RECENT)])
        .await;
    assert_eq!(recent_year(&mut fx).await, 2025);
    server.await.expect("FTPS mock panicked");
}

#[tokio::test]
async fn test_cleared_reading_falls_back_to_the_probe() {
    let mut fx = Fixture::new();
    fx.client = fx.client.with_printer_clock_probe(true);
    let mut steps = probe("213 20260202085730");
    steps.extend([
        list("/cache", ""),
        list("/", RECENT),
        Step::expect("QUIT", "221 Goodbye."),
    ]);
    let server = fx.session(steps).await;
    recent_year(&mut fx).await;
    let reading = fx
        .client
        .printer_clock_reading()
        .expect("probe read the clock");
    fx.client.disconnect_ftps().await;
    server.await.expect("FTPS mock panicked");

    fx.client.set_printer_clock_reading(Some(reading));
    fx.client.set_printer_clock_reading(None);
    // The printer rebooted and its clock now reads past `Mar 01`: the probe runs again.
    let mut steps = probe("213 20260402085730");
    steps.extend([list("/cache", ""), list("/", RECENT)]);
    let server = fx.session(steps).await;
    assert_eq!(recent_year(&mut fx).await, 2026);
    server.await.expect("FTPS mock panicked");
}

#[tokio::test]
async fn test_supplied_reading_without_probe_replaces_the_host_clock() {
    // The host clock is years past 2001, so a host-clock listing would never give 2000.
    let mut fx = Fixture::new();
    fx.client = fx.client.with_printer_clock_probe(true);
    let mut steps = probe("213 20010202085730");
    steps.extend([
        list("/cache", ""),
        list("/", RECENT),
        Step::expect("QUIT", "221 Goodbye."),
    ]);
    let server = fx.session(steps).await;
    assert_eq!(recent_year(&mut fx).await, 2000);
    let reading = fx
        .client
        .printer_clock_reading()
        .expect("probe read the clock");
    fx.client.disconnect_ftps().await;
    server.await.expect("FTPS mock panicked");

    fx.client = fx.client.with_printer_clock_probe(false);
    fx.client.set_printer_clock_reading(Some(reading));
    let server = fx
        .session(vec![list("/cache", ""), list("/", RECENT)])
        .await;
    assert_eq!(recent_year(&mut fx).await, 2000);
    server.await.expect("FTPS mock panicked");
}
