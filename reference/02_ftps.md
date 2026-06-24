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

1.  **P2S (Firmware `01.02.00.00`) TLS 1.3 Session Ticket Failure**: The embedded vsFTPd server fails to process the asynchronous session-ticket model used by TLS 1.3 on the FTPS data channel. This results in standard file transfers truncating prematurely at arbitrary chunk boundaries and returning a `426 "Failure reading network stream"` error. Connections must restrict the SSL context's maximum negotiated protocol version strictly to `TLS 1.2` to ensure synchronous session ticket resumption.
2.  **X2D (Firmware `01.01.00.00`) TLS 1.3 Handshake Failure**: Handshakes conducted over TLS 1.3 ClientHello sequences fail on Port 990 with `[SSL: WRONG_VERSION_NUMBER]`. Connections must negotiate strictly over `TLS 1.2` to establish a secure control session.
3.  **A1 Series Plaintext Data Channel Constraint**: The A1 series does not support TLS on the passive data channel due to embedded hardware limitations. To handle this, the standard `PROT P` (Private) command must not be transmitted over the secure control socket (Port 990) during connection initialization. This leaves the passive data channel in the default `PROT C` (Clear/plaintext) state while the primary command channel remains fully encrypted.

#### Physical Storage Filepaths [REF-FTPS-OPS]
The physical storage architecture utilizes a flat folder structure under the root directory:
*   `/cache`: Persistent cache directory residing on the physical MicroSD card. Used by the printer to store uploaded `.3mf` files and unpack sliced file manifests.
*   `/timelapse`: Dedicated directory for raw print records. Contains `.mp4` or `.avi` files depending on model parameters, and an internal subdirectory `/timelapse/thumbnail` containing paired `.jpg` cover sheets.
*   `/model`: Storage location for uploaded `.3mf` or standalone `.gcode` print payloads.
*   `/data`: Core calibration parameters and logs.
*   `/data/Metadata`: Stores parsed metadata (previews, model structures) extracted by the firmware.

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
*   `parts[8:]`: Reconstructed file or folder name (joined with single spaces to preserve spaces in filenames).

##### Rollover Logic
If `parts[7]` contains a time pattern (`HH:MM`), the modification year is omitted. In this scenario, the host's current calendar year is assumed. If the calculated datetime is in the future relative to the host machine's system clock (`parsed_time > current_time`), the year value must be decremented by 1 (`year = current_year - 1`) to account for rollover boundaries (e.g., parsing a December modification date in January).

#### Space Evaluation via AVBL and STAT
To query available storage capacity on the MicroSD card without performing expensive recursive directory traversals, the client must execute a direct hardware-level space query over the active control channel:
1.  **AVBL Command**: The client transmits `AVBL\r\n` to the control socket.
    *   **Successful Response**: `213 <bytes_available>\r\n` (e.g., `213 14820352000`).
2.  **STAT Command (Fallback)**: If `AVBL` returns a `500 Syntax error, command unrecognized` response (depending on older firmware lines), the client must transmit `STAT\r\n` and parse the returned status output for storage size descriptors.

---

### 2.3 Over-the-Wire Control Command Schema (The Write Stream) [REF-FTPS-XFER]

FTP commands are transmitted as ASCII strings terminated by `\r\n` over the primary control socket.

#### Handshake and Session Protection Commands
```text
USER bblp\r\n            <- Transmission of secure username
PASS <access_code>\r\n   <- Transmission of access code password
PBSZ 0\r\n               <- Set Protection Buffer Size to zero
PROT P\r\n               <- Enforces full TLS encryption on Passive Data channels
PASV\r\n                 <- Requests passive port mapping allocation
AVBL\r\n                 <- Queries available storage space on MicroSD card
```

#### Directory Mutator Commands
```text
DELE <file_path>\r\n     <- Delete a file from the server
RMD <folder_path>\r\n    <- Remove a folder from the server
MKD <folder_path>\r\n    <- Create a folder on the server
RNFR <src_path>\r\n      <- Rename from source path (paired with RNTO)
RNTO <dest_path>\r\n     <- Rename to destination path (paired with RNFR)
```

##### File Deletion Status Codes
When executing the file deletion command (`DELE`), parsers must evaluate the numeric reply code on the control socket:
*   **`250`**: Request completed successfully (the target file was deleted).
*   **`550`**: File not found / permission denied. This represents a **terminal success indicator** for cleanup operations (the file is confirmed absent from the storage directory). Retrying is futile.
*   **Other 4xx / 5xx codes**: Represent transient network timeouts, filesystem locking contentions, or authentication failures. These are retryable faults.

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

---

### 2.4 Mechanical & Firmware Quirks

#### P2S / X2D TLS 1.3 Session Close Race [REF-FTPS-CONN]
Under TLS 1.3, the FTP control server may close the connection prematurely after file transfers or throw a transient `426 "Failure reading network stream"` during close negotiations. This occurs because the TCP data channel close event races the `226` transfer confirmation packet. To handle this, the transfer must be verified via the `SIZE` command:
1.  Complete the data transmission.
2.  Terminate the data channel socket directly.
3.  Query `SIZE <remote_path>` over the control channel.
4.  If the returned size matches the source file size byte-for-byte, ignore any preceding transient network alerts and proceed with the print dispatch command. If the size mismatches, the transfer must be treated as genuinely truncated.

#### Command Channel Post-Transfer Response Synchronization
The printer's embedded FTPS server does not transmit a TLS `close_notify` shutdown alert upon completion of data channel transfers. If the connecting peer client expects a standard graceful TLS shutdown negotiation on the data socket, the session will hang indefinitely. To prevent this, the client must abruptly close the passive TCP data connection socket immediately after writing the final byte of the file payload.

Following the data channel closure, the client must block-wait on the secure control connection for the positive completion reply (`226 Transfer complete`). Because of substantial write latency on the physical MicroSD card controller, this control channel response can be delayed by up to 300 seconds as internal buffers are flushed to non-volatile flash storage.

#### MicroSD Flush Validation & 0500-C010 Exceptions [REF-FTPS-FLUSH]
The client must await the positive `226 Transfer complete` response on the control channel after closing the passive data channel before dispatching any print commands. If a print command is issued before the printer has fully flushed the file from its write buffers to physical storage, the printer's execution processor will attempt to parse an incomplete payload, triggering a physical `0500-C010 "MicroSD Card read/write exception"` on the printer panel and halting the system.
