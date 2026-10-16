# Chapter 2: Secure FTP File & Storage System

---

### 2.1 Network Boundary & Interface Parameters [REF-FTPS-CONN]

Bambu Lab printers support local file transfer via File Transfer Protocol Secure (FTPS). This system allows for the management, retrieval, and upload of print jobs, logs, and metadata directly to the on-board storage medium.

#### Connection Mechanics
*   **Target Port**: `990`
*   **Command Channel Handshake**: The raw TCP connection is established and immediately wrapped in a secure TLS session prior to issuing or receiving the standard FTP greeting. Because the channel is implicitly encrypted from the first byte, explicit handshake commands such as `AUTH TLS` must not be transmitted.
*   **Data Channel Handshake**: Active (`PORT`) mode is not supported; clients must initiate Passive (`PASV` or `EPSV`) mode. After PASV negotiation, the client commands the printer to protect the data stream using the `PROT P` command on the control channel.
*   **Session Reuse Requirements**: The printer's embedded vsFTPd daemon requires TLS session resumption on the passive data channel. The TLS handshake on the data channel TCP socket must reuse the SSL session negotiated during the primary control channel handshake. Without this session ticket reuse, vsFTPd immediately closes the data connection and returns `425 Security: Bad IP connect`.

#### Model-Specific TLS & Plaintext Encryption Constraints
Certain printer models require specific configuration of the SSL context or command negotiation parameters to prevent transport-layer or filesystem failures during file transfers:

1.  **P2S (Firmware `01.02.00.00`) TLS 1.3 Session Ticket Failure**: The embedded vsFTPd server fails to process the asynchronous session-ticket model used by TLS 1.3 on the FTPS data channel. This results in standard file transfers truncating prematurely at arbitrary chunk boundaries and returning a `426 "Failure reading network stream"` error. Connections must restrict the SSL context's maximum negotiated protocol version strictly to `TLS 1.2` to ensure synchronous session ticket resumption. **This is a firmware bug in that specific vsFTPd build, not a real ceiling on the protocol** — corroborated independently by the `bambuddy` project (reporter `@iitazz`, upstream issue #1401), which hit the exact same truncation only after upgrading its own client to a Python runtime that defaults to TLS 1.3. **The TLS 1.2 cap narrows this race, it does not close it**: `bambuddy`'s own follow-up (issue #1417) found the data-channel close can still occasionally race the final `226` confirmation even under TLS 1.2, returning a transient `426` on an otherwise-intact upload. The reliable fix is verifying the transfer via `SIZE` regardless of the final reply code, not the TLS version cap alone — see [REF-FTPS-XFER] §2.3. **The session-ticket mechanism is firmware-version-scoped at best**: it presupposes the printer negotiates TLS 1.3 at all, and `bambuddy`'s later nine-printer probe (issue #2780) found six P2S units refusing 1.3 outright — correcting their own earlier claim that "the P2S evidently does offer 1.3". On that firmware the negotiated version was already 1.2 and the cap changes nothing. Either the firmware moved between the two reports, or #1401 was fixed by something else in the same change. This is still the only one of the two caps below whose symptom a session-ticket problem could explain at all; treat it as confirmed-by-symptom, not confirmed-by-mechanism.
2.  **X2D (Firmware `01.01.00.00`) Port-990 Handshake Failure**: The implicit-FTPS handshake fails on Port 990 with `[SSL: WRONG_VERSION_NUMBER]`. Connections are capped to `TLS 1.2`, which clears the symptom. **The mechanism originally recorded here — that the failure was a response to a TLS 1.3 ClientHello — has been falsified.** `bambuddy`'s nine-printer farm measurement (issue #2780) produced three results:

    ```
    cleartext "421" banner on the TLS port
      -> [SSL: WRONG_VERSION_NUMBER] wrong version number (_ssl.c:1032)
    1.2-only server, client forced to 1.3
      -> [SSL: TLSV1_ALERT_PROTOCOL_VERSION]
    1.2-only server, an uncapped client
      -> negotiates 1.2 and connects
    ```

    The first is byte-for-byte what the field reports. So `WRONG_VERSION_NUMBER` means **the peer's first bytes were not a TLS record at all**; a version mismatch reports itself differently, and reaching a TLS-1.2-only peer needs no cap. The leading hypothesis is an FTP-level refusal sent in the clear — such as `421 Too many connections` from a printer out of connection slots.

    **The cap is kept.** The original reporter (`@vasmarfas`, bambuddy issue #1638) saw the symptom clear, and a cap costs nothing on a printer that never offers TLS 1.3. What was wrong is the recorded reasoning and the confidence it implied. Treat this as **confirmed-by-symptom with no confirmed mechanism**; bambuddy marked their own X2D entry RE-TEST WANTED for the same reason. **Resolving it needs X2D hardware**, or a packet capture of a port-990 connect showing whether the printer's first bytes are a TLS record or a cleartext FTP reply — if it is a cleartext `421`, this cap is unrelated to the fix and should be reconsidered.
3.  **A1 Series Plaintext Data Channel Constraint**: The A1 series does not support TLS on the passive data channel due to embedded hardware limitations. To handle this, the standard `PROT P` (Private) command must not be transmitted over the secure control socket (Port 990) during connection initialization. This leaves the passive data channel in the default `PROT C` (Clear/plaintext) state while the primary command channel remains fully encrypted.

#### Leaf Certificate Identity Fields [REF-FTPS-TLS-CERT]
**Confirmed on a P1S** via `bambino-cli inspect-cert` against port 990, `openssl x509 -noout -text` on the captured leaf: the certificate is **X.509v1** (`Version: 1 (0x0)`), which by definition carries no extensions field at all — there is no Subject Alternative Name extension, present or absent-with-content, only `Subject: CN=<serial>` (the printer's real serial, in the standard Bambu serial format). The signing CA certificate returned alongside it (`Issuer: C=CN, O=BBL Technologies Co., Ltd, CN=BBL CA`) is itself X.509v3.

This confirms the SAN-absent path of `verify_name_matches_leaf_cert` (`src/io/tokio/cert_verify.rs`) is the one real Bambu firmware exercises: `subject_alternative_name()` returns `Ok(None)` for a v1 cert (no extensions to parse), and the verifier falls through to CN matching, which succeeds. The hard-fail branch added in commit `7cf0d5e` — SAN extension *present* but with zero `dNSName` entries — was never observed; a v1 leaf cannot present that shape at all. That branch remains unverified against a v3 Bambu leaf (if any model ships one), but the risk that motivated filing it (a working LAN connection failing closed) does not apply to the P1S's actual cert shape.

Not yet checked on X1C, H2D, or other model lines; a v3 leaf carrying its own SAN cannot be ruled out for those without a separate capture.

#### Physical Storage Filepaths [REF-FTPS-OPS]
The physical storage architecture utilizes a flat folder structure under the root directory:
*   `/cache`: Persistent cache directory residing on the physical MicroSD card. Used by the printer to store uploaded `.3mf` files and unpack sliced file manifests. On a P1S (2026-10-16) it held four `.3mf` files (named `<name>.3mf`, not `<name>.gcode.3mf`; origin unknown) among 215 `.gcode`/`.bbl` extracts. It is the one folder the printer's screen shows as a folder: every other folder's files are flattened into the screen's root list.
*   `/timelapse`: Dedicated directory for raw print records. Contains `.mp4` or `.avi` files depending on model parameters, and an internal subdirectory `/timelapse/thumbnail` containing paired `.jpg` cover sheets. On a P1S the recordings are `.avi`, named `video_<YYYY-MM-DD_HH-MM-SS>.avi`, and each has a cover `thumbnail/<same stem>.jpg` (16 of 16, 2026-10-16).
*   `/model`: Storage location for uploaded `.3mf` or standalone `.gcode` print payloads. On a P1S it held only Bambu's preinstalled sample prints, as `.gcode`, dated 2023-09-28, and no `.3mf`.
*   `/ipcam`: On a P1S, one 4-byte file named `index` and no videos. open-bamboo-networking's FTPS stand-in for the file tunnel maps `type: "video"` here (`resolve_subtree`, `stubs/BambuSource.cpp`).
*   `/data`: Core calibration parameters and logs.
*   `/data/Metadata`: Stores parsed metadata (previews, model structures) extracted by the firmware.

**Where a running job's file is.** Measured on a P1S (2026-10-15):

*   **Sent from Bambu Studio over LAN:** the file lands at the card root, and the start command says so (`url: "ftp://<name>"`, `file: "<name>"`). Telemetry's `gcode_file` is the bare name: `benchy-multiobject-test.gcode.3mf`, `632-multiplate-test_plate_2.gcode.3mf`. In `/cache` the printer keeps only its own extracts for the job (`<name>_plate_N.gcode`, `1_<name>.gcode.bbl`), not a `.3mf`.
*   **Started from the printer's screen:** no `project_file` frame appears on the report topic. `gcode_file` and `subtask_name` are both the file's bare name, extension included, **with no folder**: a file at `/test632/deeper/level/multi2.gcode.3mf` reported `gcode_file: "multi2.gcode.3mf"`. The screen's file list shows every folder except `cache` flattened into one list, so the folder is gone there too.
*   The screen's file list also persisted a deleted file for minutes, across leaving and re-entering the list, until a power cycle.

So `/` + `gcode_file` names the job's file whenever it is at the root, which is where both slicers put it. A file placed in a folder by hand has no telemetry route to its path. `PrinterClient::job_path` uses the start command's `url` when it saw one, else `/` + `gcode_file`, confirmed with one `SIZE`, and `None` otherwise. It deliberately doesn't search the card: one `LIST` per folder (0.47 s for `/`, 2.13 s for `/cache` on this card) to cover hand-placed files only. If a hand-placed file is printed while a different file with the same name sits at the root, `job_path` returns the root file.

---

### 2.2 Over-the-Wire Telemetry Payload Schema (The Read Stream) [REF-FTPS-OPS]

Directory listings and file metadata are parsed from standard UNIX listing arrays returned by the `LIST` command over the passive data channel.

#### UNIX Listing Parser
Directory payloads are returned as CRLF-separated strings conforming to standard UNIX format:

```text
drwxr-xr-x    2 1000     1000         4096 Jun 17  2025 cache
-rw-r--r--    1 1000     1000      1632221 Jun 17 12:14 video_2026-06-17_12-12-18.mp4
```

##### Tokenization & Spacing Constraints
The printer's embedded vsFTPd server outputs variable whitespace padding (such as double spaces before year fields, e.g., `"Jun 17  2025"`). Directory listings use variable whitespace spacing as delimiters. Tokenization must split fields by one or more whitespace characters (arbitrary length) rather than relying on static character offsets.
Once tokenized, the fields correspond to:
*   `parts[0]`: Permissions/flags (a leading `d` designates a directory).
*   `parts[4]`: File size in bytes.
*   `parts[5]`: Modification month (3-letter abbreviation).
*   `parts[6]`: Modification day.
*   `parts[7]`: Modification time (`HH:MM`) or calendar year (`YYYY`).
*   `parts[8:]`: File or folder name — the raw remainder of the line after the 8th whitespace-delimited field, sliced out verbatim (not re-tokenized/rejoined) so runs of multiple consecutive spaces inside the real filename are preserved rather than collapsed to one (confirmed on a P1S).

##### Rollover Logic
If `parts[7]` contains a time pattern (`HH:MM`), the modification year is omitted and must be reconstructed against a reference clock. If the calculated datetime is in the future relative to that reference (`parsed_time > reference_time`), the year value must be decremented by 1 (`year = reference_year - 1`) to account for rollover boundaries (e.g., parsing a December modification date in January).

The correct reference is the **printer's** clock, not the host's: vsFTPd chose the `HH:MM` form over `YYYY` by comparing the file's mtime against its own clock, so that clock is the only reference the omission is meaningful against. Supplying host time to a printer whose clock is years off yields wrong years, and inconsistently so: the rollover comparison then fires on an arbitrary subset of entries rather than all or none. bambino requires the caller to pass this reference explicitly (`CurrentDateTime`) and flags every entry whose year came from it (`FtpFile::year_is_inferred`) rather than presenting a reconstruction as fact.

Recovering the printer's clock to use as that reference is itself awkward, which is why host time is the common fallback: the `HH:MM`/month/day values in any listing entry are already the printer's, but the year, the one component actually missing, is not obtainable from `LIST` at all for a recent file. `MDTM` (below) is the direct route if the firmware implements it. Confirmed on a P1S (BUG-042, `BACKLOG.md`): ESP32/FreeRTOS-class printers have no RTC battery, LAN-mode NTP sync is unreliable, and the clock restarts from a fixed base on boot (originally recorded here as the firmware build date; the `MDTM` measurements below disprove that, the base predates the build by ~2 months). Two `LIST` checks minutes apart against a fresh boot returned an identical, months-stale timestamp rather than an advancing one. This is the printer's default LAN-mode state, not a rare edge case. Unconfirmed on X1/H2-series printers with more capable AP controllers.

#### Absolute Timestamps via MDTM

`MDTM <path>` returns `213 YYYYMMDDHHMMSS`: an absolute mtime with an explicit four-digit year, no reference clock and no rollover heuristic involved. This is the only route to a file timestamp that doesn't depend on the reconstruction above, and it costs one control-channel round trip with no data channel and no write to the card.

**Confirmed on a P1S** (firmware `01.10.00.00`): `MDTM` is implemented and returns a well-formed `213 YYYYMMDDHHMMSS`, matching the `LIST` reconstruction of the same file to the minute. Unverified on A1/X1/H2/P2S.

Two probes of the same unit, five hours apart, show the clock is **not** a fixed offset from real time:

| Host time (UTC) | `MDTM` reply | Printer clock | Behind host |
| --- | --- | --- | --- |
| `2026-08-24 15:14:40` | `20260212012638` | `2026-02-12 01:26:38` | 193 days |
| `2026-08-24 20:35:36` | `20260202085730` | `2026-02-02 08:57:30` | 203 days |

The second reading is ten days *earlier* than the first despite being taken later, so the printer rebooted in between and its clock restarted from a fixed base near `2026-02-02 08:5x`. The first reading is that base plus roughly 9d16h of uptime. The reset base is not the firmware build date: `01.10.00.00` was released `2026-03-30`, nearly two months after the value the clock returns to.

The practical consequence is that a measured offset is only valid for the current power cycle. Anything that converts printer timestamps to real time must re-probe after a reboot rather than caching the correction.

The `Ok(None)` path is still required rather than optional: these builds are trimmed, and the same unit that implements `AVBL` answers `502` to `STAT` (below), so per-model absence is plausible until each is checked. A client must treat `500`/`502` as "unsupported, fall back to the `LIST` heuristic" rather than as an error; bambino's `FtpsClient::modification_time` returns `Ok(None)` on those codes and `Err` on a genuine failure such as `550` (no such file). `bambino-cli files <IP> <SERIAL> [ACCESS_CODE] clock-check` prints which branch a given printer takes.

Note the limit even when supported: `MDTM` reports what the printer believes, so an unsynced printer answers with a confidently-wrong absolute timestamp instead of an ambiguous one. It removes the reconstruction, not the clock skew.

#### Space Evaluation via AVBL and STAT
To query available storage capacity on the MicroSD card without performing expensive recursive directory traversals, the client must execute a direct hardware-level space query over the active control channel:
1.  **AVBL Command**: The client transmits `AVBL\r\n` to the control socket.
    *   **Successful Response**: `213 <bytes_available>\r\n` (e.g., `213 14820352000`).
2.  **STAT Command (Fallback)**: If `AVBL` returns a `500 Syntax error, command unrecognized` response (depending on older firmware lines), the client must transmit `STAT\r\n` and parse the returned status output for storage size descriptors. Confirmed on a P1S: `AVBL` is implemented and used normally; `STAT` itself returns `502 Command not implemented` on this firmware rather than a parseable status body — the fallback path is unreachable in practice on this unit, though it may still be needed on older firmware lines per the note above.

---

### 2.3 Over-the-Wire Control Command Schema (The Write Stream) [REF-FTPS-XFER]

FTP commands are transmitted as ASCII strings terminated by `\r\n` over the primary control socket.

#### Handshake and Session Protection Commands
```text
USER bblp\r\n            <- Transmission of secure username
PASS <access_code>\r\n   <- Transmission of access code password
PBSZ 0\r\n               <- Set Protection Buffer Size to zero
PROT P\r\n               <- Enforces full TLS encryption on Passive Data channels
TYPE I\r\n               <- Sets binary transfer mode (prevents ASCII corruption)
PASV\r\n                 <- Requests passive port mapping allocation
AVBL\r\n                 <- Queries available storage space on MicroSD card
```

**Note:** The `TYPE I` command is mandatory. RFC 959 defaults to ASCII mode, which applies line-ending transformations that corrupt binary payloads (`.3mf`, `.gcode`, timelapse videos). On A1 series models where `PROT P` is omitted (see §2.1), `TYPE I` must still be transmitted.

**Note — single-write requirement:** each command line (`<CMD> <args>\r\n`) must be sent as one contiguous write to the control-channel socket, not as separate writes for the command body and the trailing `\r\n`. Confirmed on a P1S: splitting a command across two writes (even with an immediate flush after both) causes the printer's embedded FTP daemon to desync its line parser — observed as `PASS` returning `332` instead of `230`, followed by every subsequent command returning `502`, even though the same command sequence sent as a single write logs in cleanly. This isn't RFC-mandated (TLS/TCP give no guarantee that record or segment boundaries preserve write-call boundaries anyway), but this firmware's parser is apparently sensitive to it in practice — bambino was bitten by exactly this in commit `6385019` (see `src/ftps/protocol.rs`'s `write_command`).

#### Directory Mutator Commands
```text
DELE <file_path>\r\n     <- Delete a file from the server
RMD <folder_path>\r\n    <- Remove a folder from the server
MKD <folder_path>\r\n    <- Create a folder on the server
RNFR <src_path>\r\n      <- Rename from source path (paired with RNTO)
RNTO <dest_path>\r\n     <- Rename to destination path (paired with RNFR)
```

##### Directory Mutator Reply Codes
Captured on a P1S (FTP banner `220 BBL-P003 FTP Server`, 2026-10-07, GitHub issues #392 and #613). This server sends **no reply text** — every reply is a bare code — so the code is all a client has:

| Command | Target | Reply | Effect |
|---|---|---|---|
| `MKD` | new directory | `250` (not RFC 959's `257`) | created |
| `MKD` | existing directory | `550` | none |
| `DELE` | file | `250` | deleted |
| `DELE` | missing file | `550` | — |
| `DELE` | empty directory | `250` | **directory removed** |
| `DELE` | non-empty directory | `550` | none (directory and contents remain) |
| `RMD` | empty directory | `250` | removed |
| `RMD` | missing directory | `550` | — |
| `RMD` | non-empty directory | `550` | none (contents remain) |

**`550` is not proof of absence.** It answers both a missing target and one the server refused to remove, with nothing to tell them apart. A client wanting idempotent cleanup must confirm absence separately: `FtpsClient::delete_file`/`remove_directory` list the parent directory on a `550` and succeed only if the target's name is not there. Other 4xx/5xx codes have not been characterized on this server.

#### Upload Pipeline Schema
To initiate a file upload, the client negotiates a passive data port, transmits the standard ASCII `STOR` command over the control socket, and then writes the binary payload stream over the newly established passive data channel socket.

##### Control Channel Command Sequence
The command socket transmits standard ASCII lines terminated with `\r\n`:
```text
STOR /model/print.3mf\r\n
```
The server responds with a transient code (typically `150`) indicating the data channel is opening.

##### Data Channel Binary Stream
The client writes raw binary chunks directly to the passive data channel. Utilizing a block size of 65536 bytes (64KB) is recommended to ensure smooth progress reporting under typical bandwidth constraints (~50KB/s to 200KB/s).

##### Verification
Upon closing the data socket, the client must await the positive completion code on the control socket:
```text
226 Transfer complete.\r\n
```
After receiving the `226` response (or a `426` on models affected by the TLS 1.3 close race described below), the client must unconditionally verify the remote file size by issuing `SIZE <remote_path>` on the control channel. The returned byte count must match the original upload payload length exactly. This guards against silent SD card write truncation on all models, not only the P2S/X2D TLS 1.3 race condition.

#### Download Pipeline Schema
To retrieve a file from the printer's storage, the client negotiates a passive data port, transmits the `RETR` command over the control socket, and reads the binary payload from the passive data channel.

##### Control Channel Command Sequence
```text
RETR /timelapse/video_2026-06-17_12-12-18.mp4\r\n
```
The server responds with a transient code (typically `150`) indicating the data channel is opening.

##### Data Channel Binary Stream
The client reads raw binary data from the passive data channel until the server closes the connection (EOF). The same TLS session reuse and model-specific plaintext constraints described in §2.1 apply to the data channel.

##### Verification
Upon reaching EOF on the data socket, the client must close the data channel and await the positive completion code on the control socket:
```text
226 Transfer complete.\r\n
```

##### Reading Only the Start of a File [REF-FTPS-PARTIAL]
The server offers no partial download. Measured on a P1S (2026-10-14/15, GitHub issue #629):

*   **`REST` is not implemented.** `REST <offset>` after `PASV` is answered with a bare `502`, so a download cannot start mid-file. No upstream sends it either (bambuddy `bambu_ftp.py` and ha-bambulab `bambu_client.py` only pass ftplib's default `rest=None`).
*   **`ABOR` is not implemented.** `ABOR` during a `RETR` is answered `502`, and the transfer runs on to the end of the file and its `226`.
*   **A FIN on the data channel is ignored.** After the client half-closes the data connection, the server still sends the rest of the file (1.94 MB, ~8 s, on a 1.96 MB file), then closes its side and replies `226`; the control session stays usable.
*   **An RST stops the transfer at once,** and the server then closes the control connection without a reply (no `426`). Port 990 accepted new sessions immediately afterwards and at +30 s and +130 s, across runs of six back-to-back early stops from macOS, an ESP-IDF ESP32-C6 and an embassy ESP32-C6, on files of 1.96 MB and 47 MB.

So the only partial read is `RETR` from byte 0, stopped by resetting the data connection, after which the FTPS session is over and the client must log in again. `FtpsClient::download_prefix` implements exactly that, and `FtpsClient::download_file_each` stops the same way when its sink refuses a chunk. Rechecked on a P1S (2026-10-16, #638) through the streaming read: a sink stop after 65,536 bytes of a 43,965,068-byte file reset the data channel in 0.44 s, the printer closed the session, and the next login succeeded.

##### Plate Images at the Front of a `.3mf` [REF-FTPS-3MF-FRONT]
A job's plate and pick images can be read with that partial read, because both slicers put them at the start of the archive. BambuStudio's and OrcaSlicer's `_BBS_3MF_Exporter::_save_model_to_file` (each in its own `src/libslic3r/Format/bbs_3mf.cpp`) write, in order:

1.  `[Content_Types].xml` (deflate).
2.  Every plate's `Metadata/plate_N.png` and `plate_N_small.png` (128×128), then every `plate_no_light_N.png`, then `top_N.png` and `pick_N.png` per plate. All are stored uncompressed (`_add_thumbnail_file_to_archive` passes `MZ_NO_COMPRESSION`).
3.  `Metadata/plate_N.json` (`_add_bbox_file_to_archive`, deflate).
4.  The model, G-code and configs. `slice_info.config` comes near the end.

Every entry sets general-purpose flag bit 3 (flags `0x0808`): the local header carries no CRC or sizes, and a data descriptor (optional `PK\x07\x08`, CRC-32, compressed size, uncompressed size; 8-byte sizes after a Zip64 extra field) follows the data. A front-to-back walk therefore finds a stored PNG's end after its `IEND` chunk, and a deflated entry's end at the descriptor whose compressed size equals the distance walked and which is followed by a ZIP header. `ftps::project::front_members` does this and never inflates.

Measured on a P1S (2026-10-14), a 1.96 MB OrcaSlicer-sliced `.gcode.3mf` with three objects on plate 1: `[Content_Types].xml` at 0, `plate_1.png` at 286, `plate_1_small.png` at 14924, `plate_no_light_1.png` at 17986, `top_1.png` at 20340, `pick_1.png` at 33552, `plate_1.json` at 35894, first non-`Metadata/` entry at 36292. All of these were read from the first 36 864 bytes and matched a full ZIP reader byte for byte. The Bambu Studio layout is confirmed by its exporter source and by the bundled `resources/calib/pressure_advance/*.3mf` files, from which `front_members` returns `plate_1.png` and `pick_1.png` byte-identical to Python's `zipfile` (2026-10-15). Verified on a P1S (2026-10-15) with a 2.16 MB job sliced by Bambu Studio 02.08.02.61 and sent over LAN to the card root, three objects on plate 1: same order (`plate_1.png` at 286, `plate_1_small.png` 16190, `plate_no_light_1.png` 19049, `top_1.png` 21860, `pick_1.png` 37791, `plate_1.json` 40133), every entry flag `0x0808`. `FtpsClient::read_front_members` (`bambino-cli files front`) returned `plate_1.png` and `pick_1.png` byte-identical to the same members of a full download, in 0.34 s against 11.77 s for the whole file. The pick image's dominant colours decoded to 107, 118 and 140, the plate's `identify_id`s.

**After the front section.** Both exporters then write `3D/3dmodel.model` (small on a file sent to a printer: BambuStudio's send path passes `SaveStrategy::SkipModel`, so no meshes), `Metadata/project_settings.config` (~10 KB deflated), one `Metadata/plate_N.gcode.md5` for every plate with G-code (a single loop in `_save_model_to_file`), and only then the G-code. On the P1S files the `.md5` list ended at 28 KB (plate 2 of a two-plate project: `plate_2.gcode.md5` at 28019, `plate_2.gcode` at 28128) and 52 KB (the benchy job: `plate_1.gcode.md5` at 52357). `ftps::project::gcode_plates` reads that list; `reference/03_mqtt_telemetry.md` ("P1 and A1 printers don't send `plate_idx`") says why it matters.

**Pick image to `identify_id`.** `pick_N.png` fills each object with one flat colour, and `identify_id = (B << 16) | (G << 8) | R`: BambuStudio `SkipPartCanvas::LoadPickImage` (each contour's majority colour, `SkipIdHelper` then `reverse()`) and ha-bambulab `_identify_objects_in_pick_image` both decode it this way. Background is transparent (ha-bambulab skips alpha 0) or black (BambuStudio drops alpha; id 0 is none). On the measured file the three dominant colours decode to 82, 106 and 139, the plate's `identify_id`s in `slice_info.config`, which is what `skip_objects` takes. `plate_N.json`'s `bbox_objects[].id` is **not** the `identify_id` (84, 119, 152 on the same file; BambuStudio `Plater::priv::generate_first_layer_bbox` sets it from `obj->id().id`). Object names and `label_object_enabled` are only in `slice_info.config`, out of reach of a front read.

---

### 2.4 Mechanical & Firmware Quirks

#### P2S / X2D TLS 1.3 Session Close Race [REF-FTPS-CONN]
Under TLS 1.3, the FTP control server may close the connection prematurely after file transfers or throw a transient `426 "Failure reading network stream"` during close negotiations. This occurs because the TCP data channel close event races the `226` transfer confirmation packet. To handle this, the transfer must be verified via the `SIZE` command:
1.  Complete the data transmission.
2.  Close the data channel (see *Command Channel Post-Transfer Response Synchronization* below).
3.  Query `SIZE <remote_path>` over the control channel.
4.  If the returned size matches the source file size byte-for-byte, ignore any preceding transient network alerts and proceed with the print dispatch command. If the size mismatches, the transfer must be treated as genuinely truncated.

#### Command Channel Post-Transfer Response Synchronization
The printer's embedded FTPS server does not transmit a TLS `close_notify` shutdown alert upon completion of data channel transfers. A client that *waits* for the server's `close_notify` on the data socket will therefore hang indefinitely. Sending the client's own `close_notify` is fine: after writing the final byte of the payload (or reading EOF, for `RETR`/`LIST`), the client sends a TLS `close_notify` on the data channel without waiting for a reply, bounds that close by a write timeout (bambino uses `FTPS_WRITE_TIMEOUT_SECS`, 30 s), then drops the socket. A close that fails or times out is logged and the transfer continues to the `226` read. Models whose data channel is plaintext just close the TCP socket.

*Verified 2026-09-23 on a P1S (firmware `01.10.00.00`), rustls 0.23 backend (tokio): `LIST`, `STOR` and `RETR` of 1 KiB and 5 MiB files all sent `close_notify` and received the `226` within the same second, with no close failure and byte-identical round-trips. The 5 MiB transfers took ~22 s each, all spent moving the payload, not closing the channel. The mbedtls (embassy) backend has been hardware-checked for `LIST` only; ESP-IDF has not been checked.*

Following the data channel closure, the client must block-wait on the secure control connection for the positive completion reply (`226 Transfer complete`). Because of substantial write latency on the physical MicroSD card controller, this control channel response can be delayed by up to 300 seconds as internal buffers are flushed to non-volatile flash storage.

#### MicroSD Flush Validation & 0500-C010 Exceptions [REF-FTPS-FLUSH]
The client must await the positive `226 Transfer complete` response on the control channel after closing the passive data channel before dispatching any print commands. If a print command is issued before the printer has fully flushed the file from its write buffers to physical storage, the printer's execution processor will attempt to parse an incomplete payload, triggering a physical `0500-C010 "MicroSD Card read/write exception"` on the printer panel and halting the system.

An *absent* file triggers the same exception, not a clean rejection. Confirmed on a real P1S: a `project_file` command naming a `.3mf` that does not exist on the card at all is acked `result: "success"` (the ack is receipt-only, see [REF-MQTT-ACK]), after which the execution processor fails to read it and latches `0500-C010` on the panel — seconds later, long after the ack. There is no synchronous error path for "no such file"; the only signal is the delayed panel fault.

Two consequences for clients:
*   Never dispatch `project_file` for a path that has not been confirmed present on the card. A wrong or stale filename is not a no-op.
*   `0500-C010` on its own does not indicate failing hardware. Both this case and the unflushed-write case above produce it on a perfectly healthy card, so treat it as "the printer could not read the file it was told to print", not as a card-replacement signal.

`clean_print_error` clears the latch — confirmed on a P1S, which cleared a `0500-C010` induced this way with no card reinsert and no reboot. A physical reinsert remains the fallback if the motion controller ever keeps it set.
