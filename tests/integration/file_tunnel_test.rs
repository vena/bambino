//! # Port-6000 File Tunnel Through `PrinterClient`
//!
//! Exercises `PrinterClient::file_tunnel()` against a scripted tunnel server on an in-memory
//! duplex: the capability gate, lazy connect through `.with_file_tunnel()`, and teardown. The
//! frame-level protocol is unit-tested in `src/file_tunnel/`.

use std::sync::atomic::Ordering;

use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

use bambino::client::{DummyFactory, DummyTls, PrinterClient};
use bambino::error::Error;
use bambino::models::PrinterModel;

use crate::common::client::{ACCESS_CODE, SERIAL, spawn_broker, test_identity};
use crate::common::io::{CloseCountingTlsConnector, DummyTlsConnector, MockDataStreamFactory};
use crate::common::mock_mqtt::{ReportPublisher, handle_mqtt_handshake, read_publish_payload};

const MAGIC_LOGIN: u32 = 0x0101_013F;
const MAGIC_LOGIN_ACK: u32 = 0x0001_013F;
const MAGIC_CTRL: u32 = 0x0102_013F;
const MAGIC_CTRL_REPLY: u32 = 0x0002_013F;

async fn read_frame(server: &mut DuplexStream) -> (u32, Vec<u8>) {
    let mut header = [0u8; 16];
    server.read_exact(&mut header).await.unwrap();
    let len = u32::from_le_bytes(header[0..4].try_into().unwrap()) as usize;
    let magic = u32::from_le_bytes(header[4..8].try_into().unwrap());
    let mut payload = vec![0u8; len];
    server.read_exact(&mut payload).await.unwrap();
    (magic, payload)
}

async fn write_frame(server: &mut DuplexStream, magic: u32, payload: &[u8]) {
    let mut header = [0u8; 16];
    header[0..4].copy_from_slice(&(payload.len() as u32).to_le_bytes());
    header[4..8].copy_from_slice(&magic.to_le_bytes());
    server.write_all(&header).await.unwrap();
    server.write_all(payload).await.unwrap();
}

/// Accepts the login and setup, answers one `REQUEST_MEDIA_ABILITY`, and returns what the client sent.
async fn run_mock_tunnel(mut server: DuplexStream) -> (Vec<u8>, String) {
    let (magic, login) = read_frame(&mut server).await;
    assert_eq!(magic, MAGIC_LOGIN);
    write_frame(&mut server, MAGIC_LOGIN_ACK, &[0, 0, 0, 0]).await;

    let (magic, setup) = read_frame(&mut server).await;
    assert_eq!(magic, MAGIC_CTRL);
    assert!(
        String::from_utf8(setup)
            .unwrap()
            .contains(r#""mtype":12291"#)
    );
    write_frame(
        &mut server,
        MAGIC_CTRL_REPLY,
        br#"{"mtype":12291,"sequence":0,"result":0,"reply":{}}"#,
    )
    .await;

    let (magic, request) = read_frame(&mut server).await;
    assert_eq!(magic, MAGIC_CTRL);
    write_frame(
        &mut server,
        MAGIC_CTRL_REPLY,
        br#"{"cmdtype":7,"mtype":12289,"reply":{"storage":["emmc","udisk"]},"result":0,"sequence":1}"#,
    )
    .await;
    (login, String::from_utf8(request).unwrap())
}

/// A chamber-image printer's port 6000 is the camera server, which holds a tunnel login for 60 s.
/// The gate must refuse before dialing: the empty factory would turn a dial into `Network`.
#[tokio::test]
async fn chamber_image_model_is_refused_without_dialing() {
    let mut printer = PrinterClient::new(DummyTls, DummyFactory, test_identity(PrinterModel::P1S))
        .with_file_tunnel(DummyTlsConnector, MockDataStreamFactory::empty());
    let result = printer.file_tunnel().await.map(|_| ());
    assert!(
        matches!(result, Err(Error::ModelMismatch(_))),
        "expected ModelMismatch, got {result:?}"
    );
    assert!(!printer.is_file_tunnel_connected());
}

#[tokio::test]
async fn unconfigured_tunnel_reports_not_configured() {
    let mut printer = PrinterClient::new(DummyTls, DummyFactory, test_identity(PrinterModel::P2S));
    let result = printer.file_tunnel().await.map(|_| ());
    assert!(
        matches!(result, Err(Error::NotConfigured(_))),
        "expected NotConfigured, got {result:?}"
    );
}

#[tokio::test]
async fn tunnel_connects_lazily_answers_and_closes_on_disconnect() {
    let (stream, server) = spawn_broker(run_mock_tunnel);
    let (factory, _) = MockDataStreamFactory::with_stream(stream);
    let (tls, closes) = CloseCountingTlsConnector::new();
    let mut printer = PrinterClient::new(DummyTls, DummyFactory, test_identity(PrinterModel::P2S))
        .with_file_tunnel(tls, factory);

    assert!(!printer.is_file_tunnel_connected());
    let storages = printer
        .file_tunnel()
        .await
        .expect("file_tunnel should dial, log in and set up")
        .media_ability()
        .await
        .expect("media_ability should succeed");
    assert_eq!(storages, ["emmc", "udisk"]);
    assert!(printer.is_file_tunnel_connected());

    let (login, request) = server.await.expect("mock tunnel server panicked");
    assert_eq!(&login[..8], b"bblp\0\0\0\0");
    assert_eq!(&login[8..], ACCESS_CODE.as_bytes());
    assert_eq!(
        request,
        r#"{"mtype":12289,"cmdtype":7,"sequence":1,"req":{"peer":"studio","api_version":2}}"#
    );

    printer.disconnect_file_tunnel().await;
    assert_eq!(closes.load(Ordering::SeqCst), 1);
    assert!(!printer.is_file_tunnel_connected());
}

/// Accepts the login and setup on a fresh tunnel session.
async fn accept_session(server: &mut DuplexStream) {
    let (magic, _) = read_frame(server).await;
    assert_eq!(magic, MAGIC_LOGIN);
    write_frame(server, MAGIC_LOGIN_ACK, &[0, 0, 0, 0]).await;
    let (magic, _) = read_frame(server).await;
    assert_eq!(magic, MAGIC_CTRL);
    write_frame(
        server,
        MAGIC_CTRL_REPLY,
        br#"{"mtype":12291,"sequence":0,"result":0,"reply":{}}"#,
    )
    .await;
}

/// The bytes the mock tunnel serves for a running-job member.
fn member_bytes(member: &str) -> Vec<u8> {
    format!("image:{member}").into_bytes()
}

/// Serves three `mem:/16` `get_project_file` reads, as Studio's printer does, and returns the members asked for.
async fn run_mock_tunnel_job_members(mut server: DuplexStream) -> Vec<String> {
    accept_session(&mut server).await;
    let mut members = Vec::new();
    for _ in 0..3 {
        let (magic, payload) = read_frame(&mut server).await;
        assert_eq!(magic, MAGIC_CTRL);
        let split = payload.windows(2).position(|w| w == b"\n\n").unwrap();
        let req: serde_json::Value = serde_json::from_slice(&payload[..split]).unwrap();
        let param: serde_json::Value = serde_json::from_slice(&payload[split + 2..]).unwrap();
        assert_eq!(req["req"]["path"], "mem:/16");
        assert_eq!(param["command"], "get_project_file");
        let seq = req["sequence"].as_u64().unwrap();
        let member = param["file_rel_path"].as_str().unwrap().to_owned();

        let mut status = format!(
            r#"{{"cmdtype":4,"result":1,"sequence":{seq},"reply":{{"mem_dl_param_size":12}}}}"#
        )
        .into_bytes();
        status.extend_from_slice(b"\n\n{\"result\":0}");
        write_frame(&mut server, MAGIC_CTRL_REPLY, &status).await;

        let data = member_bytes(&member);
        let mut last = format!(
            r#"{{"cmdtype":4,"result":0,"sequence":{seq},"reply":{{"offset":0,"total":{n},"size":{n},"file_md5":"{md5:x}"}}}}"#,
            n = data.len(),
            md5 = md5::compute(&data),
        )
        .into_bytes();
        last.extend_from_slice(b"\n\n");
        last.extend_from_slice(&data);
        write_frame(&mut server, MAGIC_CTRL_REPLY, &last).await;
        members.push(member);
    }
    members
}

/// A broker that completes the handshake, takes the connect-time pushall, then reports plate 2 running.
async fn run_broker_reporting_plate_2(mut server: DuplexStream) -> DuplexStream {
    handle_mqtt_handshake(&mut server).await;
    let _pushall = read_publish_payload(&mut server).await;
    ReportPublisher::new(SERIAL)
        .publish(
            &mut server,
            br#"{"print":{"command":"push_status","gcode_state":"RUNNING","gcode_file":"job.gcode.3mf","plate_idx":2,"sequence_id":"1"}}"#,
        )
        .await;
    server
}

#[tokio::test]
async fn running_job_images_come_through_the_tunnel_by_name() {
    let (mqtt_stream, _broker) = spawn_broker(run_broker_reporting_plate_2);
    let (mqtt_factory, _) = MockDataStreamFactory::with_stream(mqtt_stream);
    let (tunnel_stream, tunnel) = spawn_broker(run_mock_tunnel_job_members);
    let (tunnel_factory, _) = MockDataStreamFactory::with_stream(tunnel_stream);
    let mut printer = PrinterClient::new(
        DummyTlsConnector,
        mqtt_factory,
        test_identity(PrinterModel::P2S),
    )
    .with_file_tunnel(DummyTlsConnector, tunnel_factory);
    printer.poll_telemetry().await.expect("plate_idx report");

    let mut buf = [0u8; 256];
    let images = printer
        .read_running_job_images(&mut buf)
        .await
        .expect("tunnel route");
    assert_eq!(images.plate.map(|p| p.plate), Some(2));
    assert_eq!(
        images.plate_image,
        Some(member_bytes("Metadata/plate_2.png").as_slice())
    );
    assert_eq!(
        images.small_image,
        Some(member_bytes("Metadata/plate_2_small.png").as_slice())
    );
    assert_eq!(images.file, None, "the tunnel route never learns the file");
    assert_eq!(
        images.pick_image,
        Some(member_bytes("Metadata/pick_2.png").as_slice())
    );
    assert!(
        !printer.is_file_tunnel_connected(),
        "the routed read disconnects the tunnel when done"
    );
    assert_eq!(
        tunnel.await.expect("mock tunnel panicked"),
        [
            "Metadata/plate_2.png",
            "Metadata/plate_2_small.png",
            "Metadata/pick_2.png"
        ]
    );
}

/// A tunnel that can't be reached sends the read to the FTPS route, which here is unconfigured.
#[tokio::test]
async fn running_job_images_fall_back_to_ftps_when_the_tunnel_fails() {
    let (mqtt_stream, _broker) = spawn_broker(run_broker_reporting_plate_2);
    let (mqtt_factory, _) = MockDataStreamFactory::with_stream(mqtt_stream);
    let mut printer = PrinterClient::new(
        DummyTlsConnector,
        mqtt_factory,
        test_identity(PrinterModel::P2S),
    )
    .with_file_tunnel(DummyTlsConnector, MockDataStreamFactory::empty());
    printer.poll_telemetry().await.expect("plate_idx report");

    let mut buf = [0u8; 256];
    let result = printer.read_running_job_images(&mut buf).await.map(|_| ());
    assert!(
        matches!(result, Err(Error::NotConfigured(_))),
        "expected the FTPS route's NotConfigured after the tunnel dial failed, got {result:?}"
    );
}

/// A failed dial must leave the port-6000 config in place, so the next call retries the dial
/// rather than reporting "not configured".
#[tokio::test]
async fn failed_dial_keeps_the_config_for_a_retry() {
    let mut printer = PrinterClient::new(DummyTls, DummyFactory, test_identity(PrinterModel::P2S))
        .with_file_tunnel(DummyTlsConnector, MockDataStreamFactory::empty());
    for attempt in 1..=2 {
        let result = printer.file_tunnel().await.map(|_| ());
        assert!(
            matches!(result, Err(Error::Network(_))),
            "attempt {attempt}: expected the dial failure, got {result:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// `start_print` looks up the model of an on-printer file through the tunnel.
// **Synthetic frames:** no capture shows a printer's `LIST_INFO`/`SUB_FILE` reply for this
// read, so these test bambino's side only (#637).
// ---------------------------------------------------------------------------

use bambino::mqtt::PrintJobConfig;

fn slice_info(printer_model_id: &str) -> Vec<u8> {
    format!(
        "<config>\n  <plate>\n    <metadata key=\"index\" value=\"1\"/>\n    \
         <metadata key=\"printer_model_id\" value=\"{printer_model_id}\"/>\n  </plate>\n</config>\n"
    )
    .into_bytes()
}

fn unsupplied_job() -> PrintJobConfig {
    PrintJobConfig::new(
        "job.3mf",
        "Metadata/plate_1.gcode",
        "Test Print",
        12345,
        "textured",
    )
}

/// What the mock tunnel answers `SUB_FILE` with.
#[derive(Clone)]
enum Member {
    /// The tunnel isn't asked for a member: the lookup stops at the listing.
    NotAsked,
    Missing,
    Bytes(Vec<u8>),
}

/// Serves one `LIST_INFO` with `file_lists`, then (if asked) one `SUB_FILE`; returns the requests it saw.
async fn run_mock_listing_tunnel(
    mut server: DuplexStream,
    file_lists: String,
    member: Member,
) -> Vec<String> {
    accept_session(&mut server).await;
    let mut seen = Vec::new();
    let (magic, list) = read_frame(&mut server).await;
    assert_eq!(magic, MAGIC_CTRL);
    seen.push(String::from_utf8(list).unwrap());
    let body =
        format!(r#"{{"cmdtype":1,"result":0,"sequence":1,"reply":{{"file_lists":{file_lists}}}}}"#);
    write_frame(&mut server, MAGIC_CTRL_REPLY, body.as_bytes()).await;
    if matches!(member, Member::NotAsked) {
        return seen;
    }
    let (magic, sub) = read_frame(&mut server).await;
    assert_eq!(magic, MAGIC_CTRL);
    seen.push(String::from_utf8(sub).unwrap());
    let data = match member {
        Member::Bytes(data) => data,
        _ => Vec::new(),
    };
    let mut reply = format!(
        r#"{{"cmdtype":2,"result":0,"sequence":2,"reply":{{"path":"/m/job.3mf#Metadata/slice_info.config","size":{}}}}}"#,
        data.len()
    )
    .into_bytes();
    reply.extend_from_slice(b"\n\n");
    reply.extend_from_slice(&data);
    write_frame(&mut server, MAGIC_CTRL_REPLY, &reply).await;
    seen
}

const ONE_MODEL: &str = r#"[{"name":"job.3mf","path":"/m/job.3mf","time":1,"size":10}]"#;

/// A P2S whose tunnel serves `file_lists` and `member`, with no MQTT broker: a start that got past the check would fail with `Network`, not `ModelMismatch`.
async fn start_print_refused_by_lookup(file_lists: &str, member: Member) -> (String, Vec<String>) {
    let (tunnel_stream, tunnel) = spawn_broker({
        let file_lists = file_lists.to_owned();
        move |server| run_mock_listing_tunnel(server, file_lists, member)
    });
    let (tunnel_factory, _) = MockDataStreamFactory::with_stream(tunnel_stream);
    let mut printer = PrinterClient::new(
        DummyTlsConnector,
        MockDataStreamFactory::empty(),
        test_identity(PrinterModel::P2S),
    )
    .with_file_tunnel(DummyTlsConnector, tunnel_factory);
    let result = printer.start_print(&unsupplied_job()).await;
    let Err(Error::ModelMismatch(message)) = result else {
        panic!("expected ModelMismatch, got {result:?}");
    };
    assert!(message.contains("model_unchecked"), "{message}");
    assert!(
        !printer.is_file_tunnel_connected(),
        "the lookup disconnects"
    );
    (message.to_string(), tunnel.await.expect("mock tunnel"))
}

#[tokio::test]
async fn start_print_without_a_tunnel_is_refused_without_dialing() {
    // An empty factory turns any dial into `Network`, so `ModelMismatch` proves none happened.
    let mut printer = PrinterClient::new(
        DummyTlsConnector,
        MockDataStreamFactory::empty(),
        test_identity(PrinterModel::P1S),
    )
    .with_file_tunnel(DummyTlsConnector, MockDataStreamFactory::empty());
    let result = printer.start_print(&unsupplied_job()).await;
    assert!(
        matches!(&result, Err(Error::ModelMismatch(m)) if m.contains("no file tunnel") && m.contains("model_unchecked")),
        "{result:?}"
    );
}

#[tokio::test]
async fn start_print_publishes_when_the_listed_file_names_an_accepted_model() {
    let (mqtt_stream, broker) = spawn_broker(|mut server| async move {
        handle_mqtt_handshake(&mut server).await;
        let _pushall = read_publish_payload(&mut server).await;
        read_publish_payload(&mut server).await
    });
    let (mqtt_factory, _) = MockDataStreamFactory::with_stream(mqtt_stream);
    let (tunnel_stream, tunnel) = spawn_broker(|server| {
        run_mock_listing_tunnel(
            server,
            ONE_MODEL.to_owned(),
            Member::Bytes(slice_info("N7")),
        )
    });
    let (tunnel_factory, _) = MockDataStreamFactory::with_stream(tunnel_stream);
    let mut printer = PrinterClient::new(
        DummyTlsConnector,
        mqtt_factory,
        test_identity(PrinterModel::P2S),
    )
    .with_file_tunnel(DummyTlsConnector, tunnel_factory);

    printer
        .start_print(&unsupplied_job())
        .await
        .expect("start_print");

    let json = broker.await.expect("broker");
    assert_eq!(json["print"]["command"], "project_file");
    let seen = tunnel.await.expect("mock tunnel");
    // The card is listed with no storage label, and the member is addressed by the listed path.
    assert_eq!(
        seen[0],
        r#"{"mtype":12289,"cmdtype":1,"sequence":1,"req":{"type":"model","api_version":2,"notify":"DETAIL"}}"#
    );
    assert_eq!(
        seen[1],
        r##"{"mtype":12289,"cmdtype":2,"sequence":2,"req":{"paths":["/m/job.3mf#Metadata/slice_info.config"]}}"##
    );
    assert!(!printer.is_file_tunnel_connected());
}

#[tokio::test]
async fn start_print_refuses_when_the_member_names_another_model() {
    // N2S is the A1 file of #657.
    let (message, _) =
        start_print_refused_by_lookup(ONE_MODEL, Member::Bytes(slice_info("N2S"))).await;
    assert!(message.contains("N2S"), "{message}");
}

#[tokio::test]
async fn start_print_refuses_when_the_file_is_not_listed() {
    let (message, seen) = start_print_refused_by_lookup(
        r#"[{"name":"other.3mf","path":"/m/other.3mf","time":1,"size":10}]"#,
        Member::NotAsked,
    )
    .await;
    assert!(
        message.contains("not in the printer's model listing"),
        "{message}"
    );
    assert_eq!(seen.len(), 1, "no member is requested");
}

#[tokio::test]
async fn start_print_refuses_when_the_name_is_listed_twice() {
    let (message, _) = start_print_refused_by_lookup(
        r#"[{"name":"job.3mf","path":"/a/job.3mf","time":1,"size":10},{"name":"job.3mf","path":"/b/job.3mf","time":1,"size":10}]"#,
        Member::NotAsked,
    )
    .await;
    assert!(
        message.contains("ambiguous") && message.contains("sliced_for"),
        "{message}"
    );
}

#[tokio::test]
async fn start_print_refuses_when_the_member_is_missing() {
    let (message, _) = start_print_refused_by_lookup(ONE_MODEL, Member::Missing).await;
    assert!(
        message.contains("no Metadata/slice_info.config"),
        "{message}"
    );
}

#[tokio::test]
async fn start_print_refuses_when_the_tunnel_handshake_fails() {
    let mut printer = PrinterClient::new(
        DummyTlsConnector,
        MockDataStreamFactory::empty(),
        test_identity(PrinterModel::P2S),
    )
    .with_file_tunnel(DummyTlsConnector, MockDataStreamFactory::empty());
    let result = printer.start_print(&unsupplied_job()).await;
    assert!(
        matches!(&result, Err(Error::ModelMismatch(m)) if m.contains("connection failed") && m.contains("model_unchecked")),
        "{result:?}"
    );
}
