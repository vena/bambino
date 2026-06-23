//! # Client Coordinator Behavioral Integration Tests
//!
//! Validates the safety boundaries, temperature clamps, fan step calculations, and
//! G-code wrapping heuristics implemented inside `PrinterClient`.
//!
//! Evaluates the client against an inline, in-memory duplex stream mock to ensure
//! exact verification of the generated raw JSON payloads and raw G-code arrays.

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

use bambu_lan::client::{FanTarget, PrinterClient};
use bambu_lan::discovery::BambuModel;
use bambu_lan::error::BambuError;
use bambu_lan::io::{TimerProvider, TokioIo};
use bambu_lan::mqtt::BambuMqttClient;

// ============================================================================
// Shared Test Primitives & Handshake Mocks
// ============================================================================

struct DummyTimer;

impl TimerProvider for DummyTimer {
    async fn sleep(_duration: Duration) {
        // No-op for high-frequency in-memory tests
    }
}

/// Helper performing non-blocking bit operations to read MQTT variable-length numbers.
async fn read_var_len(stream: &mut DuplexStream) -> usize {
    let mut rem_len: usize = 0;
    let mut multiplier: usize = 1;
    loop {
        let mut byte = [0u8; 1];
        stream.read_exact(&mut byte).await.unwrap();
        let b = byte[0];
        rem_len += ((b & 127) as usize) * multiplier;
        multiplier *= 128;
        if (b & 128) == 0 {
            break;
        }
    }
    rem_len
}

/// Simulates standard MQTTS login handshakes to establish the client session.
async fn handle_mqtt_handshake(stream: &mut DuplexStream) {
    let mut header = [0u8; 1];

    // 1. Validate CONNECT packet
    stream.read_exact(&mut header).await.unwrap();
    assert_eq!(header[0], 0x10, "Expected CONNECT type identifier");
    let rem_len = read_var_len(stream).await;
    let mut payload = vec![0u8; rem_len];
    stream.read_exact(&mut payload).await.unwrap();

    // Reply with positive CONNACK confirmation (accepted)
    stream.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap();
    stream.flush().await.unwrap();

    // 2. Validate SUBSCRIBE packet
    stream.read_exact(&mut header).await.unwrap();
    assert_eq!(header[0], 0x82, "Expected SUBSCRIBE type identifier");
    let rem_len2 = read_var_len(stream).await;
    payload.resize(rem_len2, 0);
    stream.read_exact(&mut payload).await.unwrap();

    // Reply with standard SUBACK confirmation (QoS 1)
    stream
        .write_all(&[0x90, 0x03, payload[0], payload[1], 0x01])
        .await
        .unwrap();
    stream.flush().await.unwrap();
}

/// Intercepts and parses the JSON body of the next MQTT Publish packet sent by the client.
async fn read_publish_payload(stream: &mut DuplexStream) -> serde_json::Value {
    let mut header = [0u8; 1];
    stream.read_exact(&mut header).await.unwrap();
    assert_eq!(header[0], 0x32, "Expected PUBLISH with QoS 1 flags");

    let rem_len = read_var_len(stream).await;
    let mut packet = vec![0u8; rem_len];
    stream.read_exact(&mut packet).await.unwrap();

    // Reconstruct topic size to locate the payload boundary
    let topic_len = u16::from_be_bytes([packet[0], packet[1]]) as usize;
    let payload_start = 2 + topic_len + 2; // +2 for Topic len, +2 for Packet ID

    let json_bytes = &packet[payload_start..];
    serde_json::from_slice(json_bytes).unwrap()
}

// ============================================================================
// Core behavioral verification cases
// ============================================================================

#[tokio::test]
async fn test_homing_safety_interlocks() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Verify Bed-on-Z Safe Homing Command (Bare G28)
        let json = read_publish_payload(&mut server_stream).await;
        assert_eq!(json["print"]["command"], "gcode_line");
        assert_eq!(json["print"]["param"], "G28\n");
    });

    let mqtt_client = BambuMqttClient::connect::<DummyTimer>(
        TokioIo(client_stream),
        "00M000000000000",
        "12345678",
    )
    .await
    .unwrap();

    // CoreXY Bed-on-Z initialization
    let mut client_x1c = PrinterClient::new(mqtt_client, "00M000000000000", BambuModel::X1C);

    // Assert public serial and model getters expose the correct fields
    assert_eq!(client_x1c.serial(), "00M000000000000");
    assert_eq!(client_x1c.model(), BambuModel::X1C);

    // Bed-on-Z Safety Guard Verification: home_z_only_danger must return ModelMismatch
    let err_res = client_x1c.home_axes(true).await;
    assert!(matches!(err_res, Err(BambuError::ModelMismatch)));

    // Standard homing should succeed with bare G28
    client_x1c.home_axes(false).await.unwrap();

    // Bed-Slinger initialization
    let (client_stream_a1, mut server_stream_a1) = tokio::io::duplex(8192);
    let broker_task_a1 = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream_a1).await;
        // Verify Bed-Slinger homing parameters write G28 Z to the stream
        let json = read_publish_payload(&mut server_stream_a1).await;
        assert_eq!(json["print"]["command"], "gcode_line");
        assert_eq!(json["print"]["param"], "G28 Z\n");
    });

    let mqtt_client_a1 = BambuMqttClient::connect::<DummyTimer>(
        TokioIo(client_stream_a1),
        "039000000000000",
        "12345678",
    )
    .await
    .unwrap();

    let mut client_a1 = PrinterClient::new(mqtt_client_a1, "039000000000000", BambuModel::A1);

    // Bed-Slingers do not share upward bed collision hazards; G28 Z homing is permitted
    client_a1.home_axes(true).await.unwrap();

    broker_task.await.unwrap();
    broker_task_a1.await.unwrap();
}

#[tokio::test]
async fn test_kinematic_and_extrusion_moves() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Read Z move packet: must carry safety limits M211 S1 and reference coordinate wraps
        let json_z = read_publish_payload(&mut server_stream).await;
        assert_eq!(
            json_z["print"]["param"],
            "M211 S1\nM1002 push_ref_mode\nG91\nG0 Z10.00 F3000\nG90\nM1002 pop_ref_mode\n"
        );

        // Read X move packet: plain relative move G91 -> G0 -> G90
        let json_x = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_x["print"]["param"], "G91\nG0 X-15.50 F6000\nG90\n");

        // Read relative manual extrusion packet
        let json_e = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_e["print"]["param"], "M83\nG0 E10.00 F900\n");
    });

    let mqtt_client = BambuMqttClient::connect::<DummyTimer>(
        TokioIo(client_stream),
        "01P000000000000",
        "12345678",
    )
    .await
    .unwrap();

    let mut client = PrinterClient::new(mqtt_client, "01P000000000000", BambuModel::P1S);

    client.move_relative('z', 10.0, 3000).await.unwrap();
    client.move_relative('x', -15.5, 6000).await.unwrap();
    client.extrude(10.0, 900).await.unwrap();

    broker_task.await.unwrap();
}

#[tokio::test]
async fn test_thermal_guards_and_temperatures() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Bed temperature verification
        let json_bed = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_bed["print"]["param"], "M140 S60\n");

        // Nozzle temperature verification
        let json_nozzle = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_nozzle["print"]["param"], "M104 T0 S220\n");

        // Active chamber temperature verification (CoreXY target)
        let json_chamber = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_chamber["print"]["param"], "M141 S45\n");
    });

    let mqtt_client = BambuMqttClient::connect::<DummyTimer>(
        TokioIo(client_stream),
        "00M000000000000",
        "12345678",
    )
    .await
    .unwrap();

    let mut client_x1c = PrinterClient::new(mqtt_client, "00M000000000000", BambuModel::X1C);

    client_x1c.set_bed_temperature(60).await.unwrap();
    client_x1c.set_nozzle_temperature(0, 220).await.unwrap();

    // Chamber temperature should succeed on enclosed CoreXY models
    client_x1c.set_chamber_temperature(45).await.unwrap();

    // Open-frame model check
    let (client_stream_a1, mut server_stream_a1) = tokio::io::duplex(8192);
    let broker_task_a1 = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream_a1).await;
    });

    let mqtt_client_a1 = BambuMqttClient::connect::<DummyTimer>(
        TokioIo(client_stream_a1),
        "039000000000000",
        "12345678",
    )
    .await
    .unwrap();

    let mut client_a1 = PrinterClient::new(mqtt_client_a1, "039000000000000", BambuModel::A1);

    // Chamber temperature targets on open-frame models must return capability mismatch
    let err_res = client_a1.set_chamber_temperature(40).await;
    assert!(matches!(err_res, Err(BambuError::ModelMismatch)));

    broker_task.await.unwrap();
    broker_task_a1.await.unwrap();
}

#[tokio::test]
async fn test_cooling_fans_and_peripheral_switches() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Verify part cooling fan (M106 P1)
        let json_cf = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_cf["print"]["param"], "M106 P1 S127\n"); // 50% PWM

        // Left auxiliary fan verification (M106 P2)
        let json_aux = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_aux["print"]["param"], "M106 P2 S255\n"); // 100% PWM
    });

    let mqtt_client = BambuMqttClient::connect::<DummyTimer>(
        TokioIo(client_stream),
        "01P000000000000",
        "12345678",
    )
    .await
    .unwrap();

    let mut client_p1s = PrinterClient::new(mqtt_client, "01P000000000000", BambuModel::P1S);

    client_p1s
        .set_fan_speed(FanTarget::PartCooling, 50)
        .await
        .unwrap();
    client_p1s
        .set_fan_speed(FanTarget::AuxiliaryLeft, 100)
        .await
        .unwrap();

    // Verify right auxiliary cooling fan is restricted on non-X2D models
    let err_res = client_p1s
        .set_fan_speed(FanTarget::AuxiliaryRight, 80)
        .await;
    assert!(matches!(err_res, Err(BambuError::ModelMismatch)));

    // Verify right auxiliary cooling fan is supported on X2D model (using Port 10)
    let (client_stream_x2, mut server_stream_x2) = tokio::io::duplex(8192);
    let broker_task_x2 = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream_x2).await;
        let json_aux_r = read_publish_payload(&mut server_stream_x2).await;
        assert_eq!(json_aux_r["print"]["param"], "M106 P10 S204\n"); // 80% PWM
    });

    let mqtt_client_x2 = BambuMqttClient::connect::<DummyTimer>(
        TokioIo(client_stream_x2),
        "20P000000000000",
        "12345678",
    )
    .await
    .unwrap();

    let mut client_x2 = PrinterClient::new(mqtt_client_x2, "20P000000000000", BambuModel::X2D);

    client_x2
        .set_fan_speed(FanTarget::AuxiliaryRight, 80)
        .await
        .unwrap();

    broker_task.await.unwrap();
    broker_task_x2.await.unwrap();
}

#[tokio::test]
async fn test_queue_lifecycle_control_blocks() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        let json_pause = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_pause["print"]["command"], "pause");

        let json_resume = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_resume["print"]["command"], "resume");

        let json_stop = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_stop["print"]["command"], "stop");
    });

    let mqtt_client = BambuMqttClient::connect::<DummyTimer>(
        TokioIo(client_stream),
        "01P000000000000",
        "12345678",
    )
    .await
    .unwrap();

    let mut client = PrinterClient::new(mqtt_client, "01P000000000000", BambuModel::P1S);

    client.pause_print().await.unwrap();
    client.resume_print().await.unwrap();
    client.stop_print().await.unwrap();

    broker_task.await.unwrap();
}

#[tokio::test]
async fn test_peripheral_signals_and_climate_controls() {
    let (client_stream, mut server_stream) = tokio::io::duplex(8192);

    let broker_task = tokio::spawn(async move {
        handle_mqtt_handshake(&mut server_stream).await;

        // Verify set_airduct_mode (set_airduct command, modeId = 0)
        let json_airduct = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_airduct["print"]["command"], "set_airduct");
        assert_eq!(json_airduct["print"]["modeId"], 0);

        // Verify set_prompt_sound (print_option command, sound_enable = true)
        let json_sound = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_sound["print"]["command"], "print_option");
        assert_eq!(json_sound["print"]["sound_enable"], true);

        // Verify set_buzzer_mode (buzzer_ctrl command, mode = 2)
        let json_buzzer = read_publish_payload(&mut server_stream).await;
        assert_eq!(json_buzzer["print"]["command"], "buzzer_ctrl");
        assert_eq!(json_buzzer["print"]["mode"], 2);
    });

    let mqtt_client = BambuMqttClient::connect::<DummyTimer>(
        TokioIo(client_stream),
        "01P000000000000",
        "12345678",
    )
    .await
    .unwrap();

    let mut client = PrinterClient::new(mqtt_client, "01P000000000000", BambuModel::P1S);

    // Recirculate (cooling) damper path -> modeId = 0
    client.set_airduct_mode(true).await.unwrap();
    client.set_prompt_sound(true).await.unwrap();
    client.set_buzzer_mode(2).await.unwrap();

    broker_task.await.unwrap();
}
