# Chapter 8: Port-6000 File Service

---

### 8.1 Network Boundary & Sources [REF-FILESVC]

Port 6000 on newer printers carries a TLS file service besides (or instead of) the chamber-image JPEG stream [REF-CAM-BINARY]. It lists, reads, deletes and uploads files on the printer's storage, reads individual members of a stored `.3mf` without downloading it, and serves in-memory files such as the running job's pick images and a camera preview. It reaches internal/eMMC storage, which FTPS on port 990 does not show. Newer firmware (H2D `01.03`, H2S `01.02`, P2S, X2D) can keep the print file there, announced by a `brtc://emmc/<name>` URL in the print command, and then FTPS may have no copy. How often that happens varies: bambuddy found the P2S and H2C with nothing on FTPS, but an H2D on `01.03.00.00` with a card inserted still had each such file under `/cache` (`print_storage.py`, bambuddy #2780 and #2856).

bambino's client is `src/file_tunnel/` (`FileTunnelClient`, `PrinterClient::file_tunnel`). It implements every request below; notifies are read and skipped. `SUB_FILE` reads one member per request, without `zip`, and `FILE_DEL` deletes one file per request. `PrinterClient::read_running_job_images` routes the running job's images through the `mem:/16` read before FTPS. Nothing has been run against a printer that serves the tunnel; the upload's pipelining is the least certain part (§8.4).

Bambu Studio reaches this service as `bambu:///local/<ip>?port=6000&user=bblp&passwd=<access code>` through the closed `libBambuSource` (`Bambu_Create`/`Bambu_Open`/`Bambu_StartStreamEx(tunnel, 0x3001)`/`Bambu_SendMessage`/`Bambu_ReadSample`). Off-LAN, the same JSON bodies travel over a cloud-minted TUTK session with different framing; bambino is LAN-only and ignores that path.

**Sources and what each one establishes:**

| Source | Layer | Notes |
|---|---|---|
| BambuStudio `src/slic3r/GUI/Printer/PrinterFileSystem.{h,cpp}` | JSON request/reply bodies, command and result codes, client flow | Authoritative for the JSON layer. The framing lives in the closed plugin, so Studio cannot speak to it. |
| ha-bambulab `custom_components/bambu_lab/pybambu/media_sources.py`, `Tcp6000MediaSource` | Framing, handshake, LIST_INFO / FILE_DOWNLOAD / REQUEST_MEDIA_ABILITY | Working client, used where FTPS hides the file. |
| open-bamboo-networking (ClusterM, commit `fbf8ff2`): `src/tunnel_local.cpp`, `src/tunnel_upload.cpp`, `research/06.04-port-6000.md`, `research/09.06-commands.md` | Framing, handshake acks, reject frame, upload pipelining, `mem:/` reply stream, ability reply | A drop-in replacement for Bambu's network plugin, reverse-engineered from MITM and Frida captures of the stock plugin against a **P2S** and an **X2D**. |
| OpenBambuAPI `lan-file-tunnel.md` | — | **Contradicted on framing** by both working clients above (it describes a 64-byte auth packet and `cmdtype` in the header). Don't use it for the wire format. It also calls `SUB_FILE` a change subscription, which Studio's use contradicts (see 8.4). It credits the command names to a `LocalControlTunnel.cpp` in "bambu_net_oss"; no such file exists in grumpydevelop/bambu_network_oss (commit `6d43a3e`), which implements MQTT, SSDP and FTPS only and has no port-6000 code. |
| bambuddy | — | Has no port-6000 file client. |

**Confidence.** The JSON layer comes from BambuStudio. The framing and handshake come from two independent working clients, ha-bambulab and open-bamboo-networking, which agree byte for byte. bambuddy has nothing to compare against, so this falls short of the BambuStudio-plus-bambuddy bar in the `backlog` skill's `evidence.md`. A bambino implementation should still follow it, since both clients that work agree. Details that only open-bamboo-networking reports are marked **(OBN only)**.

---

### 8.2 Transport, Framing & Handshake

**Transport.** TCP 6000, implicit TLS from the first byte. The leaf certificate is `CN=<serial>`, under the same trust and SNI rules as the other LAN TLS ports [REF-NET-SECURE].

**Frame header**, 16 bytes, little-endian, before every payload in both directions:

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | `payload_len`: bytes after this header (may be 0) |
| 4 | 4 | `magic` (table below) |
| 8 | 4 | `seq`: per-frame counter. Both clients seed it randomly and increment it per frame sent. It is not the JSON `sequence`. |
| 12 | 4 | zero |

**Magic values.** Byte 6 is the subchannel (`01` login, `02` control). Byte 7 is the direction (`01` client to printer, `00` printer to client).

| Magic (LE u32) | Direction | Purpose |
|---|---|---|
| `0x0101013F` | client to printer | Login |
| `0x0001013F` | printer to client | Login ack, typically 4 zero bytes **(OBN only)**. ha-bambulab reads one frame and ignores its contents. |
| `0x0102013F` | client to printer | Setup and every request |
| `0x0002013F` | printer to client | Setup reply, replies, streamed data **(OBN only)**. ha-bambulab doesn't check reply magics. |
| `0x0003013F` | printer to client | Reject: `payload_len` 8, payload `FF FF FF FF 00 00 00 00`, then the printer closes **(OBN only)**. Seen when a client sends the chamber-image 80-byte `0x3000` auth to a file-service session. |

**Handshake:**

1. **Login.** One frame with magic `0x0101013F` and a 16-byte payload: the username (`bblp`) then the LAN access code, each ASCII, truncated or NUL-padded to exactly 8 bytes. This is not the chamber-image 80-byte auth [REF-CAM-BINARY].
2. **Setup.** One control frame carrying this compact JSON:

   ```json
   {"sequence":0,"mtype":12291,"req":{"t_av":1,"mtype":12289,"peer_t":3,"pid":"<8 hex>","ver":"02.03.00.00"}}
   ```

   `12291` is `0x3003` (setup). `12289` is `0x3001`, Studio's `CTRL_TYPE`. Both clients send exactly these keys. ha-bambulab sends `pid` as 8 hex digits of its frame counter and `ver` as `02.03.00.00`. **(OBN only)**: stock Studio fills `pid`/`ver` from the URL's `cli_id`/`cli_ver`, the stock upload path sends `t_av` 0 with empty `pid`/`ver`, and the printer answers both forms the same way.
3. **Setup reply.** `{"mtype":12291,"sequence":0,"result":0,"reply":{}}`. `result == 0` means ready. Both clients treat anything else as failure.

---

### 8.3 Request & Reply Envelope

**Request** (control frame payload):

```json
{"mtype":12289,"cmdtype":<int>,"sequence":<int>,"req":{...}}
```

Studio builds `{"cmdtype","sequence","req"}`, and the plugin prepends `"mtype":12289` on the wire. Both working clients do the same. A request that carries binary (upload chunks, `mem:/` parameters) appends `\n\n` and then the binary **inside the same payload**.

**Reply:**

```json
{"cmdtype":<int>,"mtype":12289,"sequence":<int>,"result":<int>,"reply":{...}}
```

The reply may be followed by `\n\n` and binary in the same payload. Studio's `PrinterFileSystem::HandleResponse` treats everything up to the first `\n\n` as JSON and passes the rest on as data. ha-bambulab finds the end of the JSON by brace matching, then skips `\n\n` (or `\r\n\r\n`).

**Notify** (printer-initiated). There is no `result` key. `cmdtype` ≥ `0x100` and the body is in `notify`. Studio routes these by `cmdtype - 0x100`.

**Matching.** Studio matches replies on `sequence` alone. It runs one request at a time and keeps its own callback queue. The ha-bambulab and open-bamboo-networking clients match on `sequence` **and** `cmdtype` and discard anything else. That is the safer rule, because a reused session can carry a stale reply to an earlier command **(OBN only, observed on the upload path)**.

**Multi-frame replies.** A streamed reply sends several frames under one `sequence`: `result` 1 (`CONTINUE`) on each frame but the last, then `0` on the final frame or an error code. If the client abandons a stream before it finishes, Studio sends `TASK_CANCEL` for that `sequence`.

**Result codes** (`PrinterFileSystem.h`):

| Code | Name | Code | Name |
|---|---|---|---|
| 0 | `SUCCESS` | 14 | `FILE_READ_WRITE_ERR` |
| 1 | `CONTINUE` | 15 | `FILE_CHECK_ERR` |
| 2 | `ERROR_JSON` | 16 | `FILE_TYPE_ERR` |
| 3 | `ERROR_PIPE` | 17 | `STORAGE_UNAVAILABLE` |
| 4 | `ERROR_CANCEL` | 18 | `API_VERSION_UNSUPPORT` |
| 5 | `ERROR_RES_BUSY` | 19 | `FILE_EXIST` |
| 6 | `ERROR_TIME_OUT` | 20 | `STORAGE_SPACE_NOT_ENOUGH` |
| 10 | `FILE_NO_EXIST` | 21 | `FILE_CREATE_ERR` |
| 11 | `FILE_NAME_INVALID` | 22 | `FILE_WRITE_ERR` |
| 12 | `FILE_SIZE_ERR` | 23 | `MD5_COMPARE_ERR` |
| 13 | `FILE_OPEN_ERR` | 24 / 25 | `FILE_RENAME_ERR` / `SEND_ERR` |

Studio also uses some of these for its own client-side failures (`ERROR_PIPE`, `ERROR_CANCEL`, `ERROR_TIME_OUT`), so the enum alone doesn't say which ones the printer sends. Two values outside the enum have been reported. `-9203` came from a P2S when a client broke the upload pipelining rule in 8.4 **(OBN only)**. `result: 2` with `cmdtype: 0xFFFFFFFF` came back for `LIST_INFO` on H2S `01.02.00.00` (OpenBambuAPI). That second report used OpenBambuAPI's contradicted framing, so `ERROR_JSON` there more likely reflects a malformed request than a retired command (inferred).

---

### 8.4 Commands

`cmdtype` values from `PrinterFileSystem.h`: `LIST_INFO 1`, `SUB_FILE 2`, `FILE_DEL 3`, `FILE_DOWNLOAD 4`, `FILE_UPLOAD 5`, `REQUEST_MEDIA_ABILITY 7`, `LIST_CHANGE_NOTIFY 0x100`, `LIST_RESYNC_NOTIFY 0x101`, `TASK_CANCEL 0x1000`. Request shapes below are Studio's unless marked otherwise.

#### `REQUEST_MEDIA_ABILITY` (7): which storages exist

```json
{"peer":"studio","api_version":2}
```

Studio reads `reply.storage`, an array of storage labels. **(OBN only)**: a P2S answered `["emmc","udisk"]`. The stock upload path sends `{"api_version":3,"peer":"studio","peer_t":3}`, and an X2D answered `{"allow_internal_model_download":true,"api_version":3,"storage":["emmc","udisk"],"upload_storage":["emmc","udisk"]}`. ha-bambulab also accepts a bare array and the keys `upload_storage`/`storage_list`/`storages`, defensively. Probe this rather than guessing storages.

**Storage labels** seen in `storage` fields: `emmc`, `internal` (internal); `udisk`, `sdcard`, `usb`, `external`, and an absent or empty value (external). Studio's file browser sends `storage` only when its panel has one selected (`m_file_storage`). **(OBN only)**: none for the External tab, `"internal"` for the Internal tab.

#### `LIST_INFO` (1): list files

```json
{"type":"timelapse"|"video"|"model","storage":"<label, optional>","api_version":2,"notify":"DETAIL"}
```

The reply is `reply.file_lists[]` with `name`, `path`, `time` (Unix seconds), `size`. ha-bambulab also reads an optional per-entry `storage`. Studio rejects a `video`/`model` listing that comes back without `path`, as `FILE_TYPE_ERR` ("old printer that always return timelapses"). `path` is what every later command addresses a file by. **(OBN only)**: `type:model` lists `.3mf` files only.

#### `SUB_FILE` (2): read members of a stored file without downloading it

This is not a subscription. It reads named members out of a file that `LIST_INFO` listed:

```json
{"paths":["<file.path>#Metadata/plate_1.png", "<file.path>#Metadata/slice_info.config"],"zip":true,"storage":"<label, optional>"}
```

- A member is addressed as `<path from LIST_INFO>#<member path inside the zip>`. The member `#thumbnail` is a video's thumbnail.
- Studio's `FetchModel` asks for `_rels/.rels`, `3D/3dmodel.model`, `Metadata/model_settings.config`, `Metadata/slice_info.config`, `Metadata/project_settings.config` and each `plate_thumbnail_*` member, with `"zip": true`. The reply is then a zip archive containing those members. Studio's `UpdateFocusThumbnail2` requests one image member at a time without `zip`.
- An older form, `{"files":[<name>, ...]}`, asks for thumbnails by bare name (Studio's `OldThumbnail` path).
- **(OBN only)**: its client also sends `"api_version":2,"peer":"studio"`, and asks for `#Metadata/plate_<n>.png`, then `#Metadata/plate_no_light_<n>.png`, then `#thumbnail`, to get a plate image.

Each reply frame's `reply` carries `path` (the requested `file#member`), `size`, an optional `mimetype` and `thumbnail`, and `continue: true` when more fragments of the **same** member follow. The member's bytes follow `\n\n`. `size == 0` means the member is missing. `result` stays `CONTINUE` until the last member.

This needs the file's `path`, so reaching the running job means first finding its file in `LIST_INFO`.

#### `FILE_DOWNLOAD` (4): download a whole file

```json
{"path":"<path>","offset":0}
```

or `{"file":"<name>","offset":0}` when `LIST_INFO` gave no `path`. Each reply frame carries `reply.offset` (where this fragment starts), `reply.total`, `reply.size` (fragment length) and the fragment after `\n\n`. The last frame adds `reply.file_md5`, the hex MD5 of the whole file, compared case-insensitively. Studio checks both `total` and the MD5.

**`mem:/<index>`: in-memory files.** The same command with `path: "mem:/<index>"` reads a file the printer holds in memory, parameterised by a JSON blob after `\n\n`:

```text
{"mtype":12289,"cmdtype":4,"sequence":N,"req":{"path":"mem:/16","offset":0,"mem_dl_param_size":<len(blob)>}}
\n\n{"sequence_id":1,"version":1,"peer_host":"studio","command":"get_project_file","file_rel_path":"Metadata/pick_1.png"}
```

- **`mem:/16` with `get_project_file`** reads a member of **the running job's** `.3mf` by relative name, with no path. Studio's skip-objects dialog (`PartSkipDialog::DownloadPartsFile`, then `PrinterFileSystem::GetPickImages`/`GetPickImage`/`DownloadRamFile`) uses it for three members: `Metadata/pick_<plate>.png`, `Metadata/model_settings.config` and `Metadata/slice_info.config`. `<plate>` is the plate index the printer reports (`plate_idx`). Where the service exists, this is a path-free alternative to bambino's FTPS route (`PrinterClient::job_file` and `read_plate_images`' front-of-file read, #630, #632 and #639). bambino also asks it for `Metadata/plate_<plate>.png` and `Metadata/plate_<plate>_small.png`; that those are served the same way is inferred, not sourced. It still needs the plate number, which P1/A1 telemetry doesn't send (see `PrinterClient::job_plate`).
- **`mem:/26`** is the printer preview image. Studio's `MediaPlayCtrl::start_device_image_flow` fetches it through `FileTransferObject::DownloadMemFile`. **(OBN only)**: the stock plugin sends the same `get_project_file` blob with an empty `file_rel_path` and `sequence_id` equal to the request's `sequence`, and the X2D returned a 1920×1080 JPEG.

The first reply frame carries `reply.mem_dl_param_size` and the printer's own JSON blob of that length after `\n\n`, for example `{"result":0}`. That blob is not file data. Studio treats a missing `result`, `result == 1`, or `size == 0` in it as failure. The data frames and the final `file_md5` follow as for a normal download. **(OBN only)**: on a P2S, a 73 KiB JPEG arrived as 20 KiB fragments carrying `frag_id`, with the final frame at `result` 0.

#### `FILE_UPLOAD` (5): chunked upload

1. Init (JSON only): `{"type":"model","storage":"<label>","path":"<name>","total":<bytes>}`. Read the reply before sending any data. `result` 1 (`CONTINUE`) or 19 (`FILE_EXIST`; continue, it overwrites) carries `reply.chunk_size` in **KiB** and `reply.offset` (resume point).
2. Chunks, all on the init's `sequence`: `{"frag_id":<n>,"offset":<bytes>,"size":<bytes>}` + `\n\n` + bytes. The last chunk adds `"file_md5"`, the lowercase hex MD5 of the whole file.
3. One final reply, `result` 0.

**(OBN only), P2S**: send every chunk back to back without reading between them. Waiting for a per-chunk reply, or polling the socket mid-pipeline, gets `-9203` for files over 255 KiB. The printer sends no per-chunk replies, so read exactly one final reply. A P2S reported `chunk_size` 255, and an X2D 255 over LAN. `storage: "emmc"` uploads land in `/userdata/model/history/<path>` on the X2D. Don't send `FILE_DEL` before an upload: `FILE_EXIST` already overwrites.

#### `FILE_DEL` (3)

`{"paths":[<path>, ...]}`, or `{"delete":[<name>, ...]}` when there's no `path`. Studio sends at most 64 per request. **(OBN only)**: its client adds `storage`, and a P2S's `FILE_DEL` with `storage:"emmc"` sometimes reports paths under `/media/usb0/timelapse/`.

#### `TASK_CANCEL` (0x1000)

`{"tasks":[<sequence>, ...]}`. The reply's `reply.tasks` lists the sequences actually cancelled.

#### Notifies

`LIST_CHANGE_NOTIFY` (0x100) means the file list changed. `LIST_RESYNC_NOTIFY` (0x101) means the printer reset its index. Both are printer-initiated, with no request.

---

### 8.5 Which Printers Serve It

**Serves it:** P2S and X2D (open-bamboo-networking captures, X2D `01.02.00.00`). H2D and H2S are the printers ha-bambulab's client exists for, to reach internal storage FTPS doesn't show. The X1C, H2C and H2D Pro advertise it with `fun` bit 31 (below), but nobody has exercised it on them. **Unknown:** X1, X1E.

**Doesn't serve it: every chamber-image printer (A1, A1 Mini, A2L, P1P, P1S).** On a P1S running `01.10.00.00` (2026-10-15), with no camera client attached:

| Sent after TLS | Printer's response |
|---|---|
| The 8.2 login frame alone | Nothing within 5 s |
| Login and setup back to back (155 bytes) | Nothing within 10 s, connection held open |
| Login and setup back to back, long wait | Closed 60 s after the TLS handshake, having sent nothing |
| The 80-byte `0x3000` chamber auth, before and after the runs above | A JPEG frame within about 1 s |

The P1S's port 6000 is the chamber-image server only. It doesn't recognise the file-service login, and it doesn't send the `0x0003013F` reject the P2S file service uses. It just holds the connection until its 60 s idle close. The other chamber-image models are recorded as not serving it without a probe of their own. They share the P1S's port-6000 server: the same `0x3000` auth, the same one-connection limit and redial stall (bambuddy `fix(camera) #2521`, recorded in `src/camera/binary.rs`), and the same `ipcam` key set in every capture (below). The P2S file service, by contrast, accepts several simultaneous sessions **(OBN only)**.

**Telemetry can't detect it, and Studio's gate doesn't match the wire.** The file browser opens a LAN session only when the MQTT report has `print.ipcam.file.local == "local"` (`MachineObject::parse_json`, `MediaFilePanel::fetchUrl`). Its siblings are `file.remote` (`none`/`tutk`/`agora`/`tutk_agaro`) and `file.model_download`. Without them, Studio shows "Browsing file in storage is not supported in current firmware" and never dials. **No LAN capture of any model carries `ipcam.file`.** That covers ha-bambulab's mocks for A1, A2L, P1P, P2S, X1C, X2D, H2C, H2D, H2D Pro and H2S, bambino's P1S captures, OpenBambuAPI's `mqtt.md` example, and bambuddy's virtual-printer report. The P2S and X2D omit it and serve the service anyway, which is why open-bamboo-networking injects the block (`try_inject_ipcam_file_local` in `src/agent.cpp`). The `ipcam` key that does split the two groups in those captures is `rtsp_url`. It is present (a URL or `"disable"`) on every RTSPS model and absent on every chamber-image model. That is the camera split bambino's quirks already carry as `camera_protocol()`.

**`fun` bit 31 is the capability flag: `is_support_brtc`.** Studio declares it `bool is_support_brtc{false}; // fun[31], support tcp and upload protocol` (`DeviceManager.hpp`). It reads it in `MachineObject::parse_new_info`, numbering bits from the least-significant end of the hex string as `DevUtil::get_flag_bits` does. Studio gates two uses of this service on it:

- **Send to Printer** (`SendToPrinterDialog`). With the bit set, `update_show_status` opens an `ft_*` tunnel to `bambu:///local/<ip>?port=6000…` (`GetConnection`) and `on_ok` uploads through `CreateUploadFileJob`, the chunked `FILE_UPLOAD`. Without it, or once the tunnel fails, Studio uploads over FTPS (`SendJob`). The fallback works like this: `GetConnection` arms an 8 s timer that marks the connection failed, and on a LAN printer `ChangeConnectMethod` then switches straight to FTP.
- **The skip-objects button.** `PrintingTaskPanel::enable_partskip_button` enables it only when `is_support_brtc` is set. The dialog then reads the pick image through `mem:/16`.

With the bit absent, Studio never uses the service. P1 and A1 firmware send no `fun` at all (03 §3.2.1, "Capability Bitfields Are Absent on P1 and A1"), so the bit is false there, which matches the P1S probe. In the captures above, bit 31 is set on every RTSPS model with a `fun` field: X1C (`01.09.00.01`), H2C, H2D, H2D Pro, H2S, P2S and X2D. It is clear on the A2L, the one chamber-image capture that has `fun`. So the X1C, H2C and H2D Pro advertise the service too, though nobody has exercised it on them over the wire. The bit's meaning rests on BambuStudio alone. bambuddy doesn't parse it, though it independently records that H2-series, P2S and X2D firmware receive Studio's uploads over port 6000 (`print_storage.py`, `brtc://emmc/<name>` dispatch URLs).

**`ipcam.brtc_service` is not the gate.** ha-bambulab's X1C, H2 and P2S mocks report `"brtc_service": "enable"`, but no source says what it gates. open-bamboo-networking (`research/12.01.05-fields-camera-ai.md`) records two unranked readings. One is an off-LAN "Bambu WebRTC" camera transport, from third-party X1C telemetry seen alongside `DevSignal.bambu.com` signalling. The other is this port-6000 storage service. What "brtc" stands for is unknown. Whatever it means, the port-6000 service is plain TLS over TCP with JSON frames: it has no SDP, ICE, STUN or DTLS-SRTP, so it isn't WebRTC.

The other file-adjacent bits Studio decodes describe features, not the service: `fun` bit 28 (internal timelapse), `fun2` bit 0 (print with eMMC), `fun2` bit 17 (internal model storage) and `cfg` bit 42 (preview image, `mem:/26`). They are set mainly on the X2D. The P2S serves the service with `fun2` bit 17 clear.

**Skip objects** doesn't consult `ipcam.file`. Once the button is enabled (above), `PartSkipDialog::fetchUrl` dials port 6000 in LAN mode and falls back to TUTK on failure. Studio also parses `fun` bit 49 as `is_support_partskip`.

---

### 8.6 How bambino Uses It

- **Transport.** `RawStreamFactory` dials port 6000, `TlsConnector` wraps it with the serial as SNI, and the protocol runs over `AsyncIo`. `PrinterClient` uses the camera's connector slot (`with_file_tunnel()`), since no printer serves both on port 6000.
- **Allocation.** Each frame is read whole before parsing, so a payload over `DEFAULT_MAX_FRAME_PAYLOAD` (4 MiB) is refused. Lower the cap on an embedded heap (`with_file_tunnel_max_frame_size`). Downloads stream to a caller sink, and a fragment that would run past the reply's `total` is refused.
- **Integrity.** Every download checks `total` and `file_md5`, using the `md5` crate.
- **Sequencing.** One request in flight per session, matched on `sequence` and `cmdtype`. A failure `result` read in full (`Error::TunnelReply`) leaves the session usable. When the client stops a stream itself (a sink error, a caller's size limit, a refused `mem:/` read), it sends `TASK_CANCEL` and skips the stream's in-flight frames until the cancel's reply, as Studio does; a transport failure, a corrupt frame, or a cancel that gets no reply poisons the session.
- **Uploads** are an opt-in call (`upload(storage, name, data)`); the existing FTPS print path is unchanged. The caller names the storage, and a job uploaded to `emmc` is started with `PrintJobConfig::from_internal_storage()`, which sends `url: "brtc://emmc/<name>"`. The MD5 is of the whole file, per open-bamboo-networking.
- **Capability detection.** `Capabilities::file_tunnel_support()` reads `fun` bit 31. A reported bit decides. Without one, a chamber-image model is refused without dialing, since a dial would hold its single port-6000 connection for 60 s. Any other model is tried, which differs from Studio's default of false: every captured RTSPS model that sends `fun` sets the bit. A failed handshake (a `0x0003013F` reject, a close, or the 8 s timeout per handshake reply, Studio's own budget) is the caller's signal to use FTPS. `ipcam.file` is never sent, so it can't be used.
