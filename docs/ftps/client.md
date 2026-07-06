**bambino > ftps > client**

# Module: ftps::client

## Contents

**Structs**

- [`BambuFtpsClient`](#bambuftpsclient) - Lightweight, high-reliability implicit FTPS client running on top of abstract I/O traits.

---

## bambino::ftps::client::BambuFtpsClient

*Struct*

Lightweight, high-reliability implicit FTPS client running on top of abstract I/O traits.

**Poisoning invariant:** if a data-channel transfer (`list_directory`/`upload_file`/
`download_file`) fails after the server has sent its `150`/`125` "opening data connection"
reply but before the matching final reply (`226`/etc.) has been read off the control channel,
the control channel is left mid-response. Reusing the client at that point risks a later,
unrelated command silently reading the stale trailing reply instead of its own. To make this
safe, the client sets `poisoned = true` on every such error path (and unconditionally in
`disconnect()`); every public method checks the flag first and returns
[`BambuError::ProtocolViolation`] immediately if set. A poisoned client must be discarded —
reconnect via a fresh [`BambuFtpsClient::connect`] call instead of reusing the instance.

**`FtpsTimer`** bounds every read against a per-call wall-clock deadline (see
`FTPS_READ_TIMEOUT_SECS`/`FTPS_TRANSFER_CONFIRM_TIMEOUT_SECS` in `protocol.rs`) — owned
independently of whatever `Timer` a `PrinterClient` that hands out this client is using,
since `PrinterClient::storage()` hands out direct `&mut BambuFtpsClient` access rather than
mediating every method call the way it does for MQTT/camera (no call site to thread
`&self.timer` through). Defaults to `DummyTimer` (unbounded, matching this crate's existing
`DummyTimer` convention) for direct (non-`PrinterClient`) callers that don't supply one.

**Generic Parameters:**
- RawIO
- Tls
- Factory
- FtpsTimer

**Methods:**

- `fn connect(raw_control: RawIO, tls_connector: Tls, data_factory: Factory, model: BambuModel, ip: &str, access_code: &str, timer: FtpsTimer) -> Result<Self, BambuError>` - Establishes the secure control channel, performs login handshakes, and configures security properties.
- `fn list_directory(self: & mut Self, remote_path: &str, current_year: i32, current_month: u8, current_day: u8, current_hour: u8, current_minute: u8) -> Result<Vec<FtpFile>, BambuError>` - Queries the storage server for raw directory listings and parses their structures.
- `fn get_file_size(self: & mut Self, remote_path: &str) -> Result<u64, BambuError>` - Queries the exact size of a file stored on the printer's MicroSD card.
- `fn delete_file(self: & mut Self, remote_path: &str) -> Result<(), BambuError>` - Removes a targeted file from non-volatile storage.
- `fn upload_file(self: & mut Self, remote_path: &str, data: &[u8]) -> Result<(), BambuError>` - Uploads a binary payload directly to MicroSD card storage.
- `fn download_file(self: & mut Self, remote_path: &str) -> Result<Vec<u8>, BambuError>` - Downloads the contents of a remote file from MicroSD storage via the RETR command.
- `fn create_directory(self: & mut Self, path: &str) -> Result<(), BambuError>` - Creates a directory on the printer's MicroSD storage.
- `fn remove_directory(self: & mut Self, path: &str) -> Result<(), BambuError>` - Removes a directory from the printer's MicroSD storage.
- `fn rename_file(self: & mut Self, from: &str, to: &str) -> Result<(), BambuError>` - Renames a file or directory on the printer's MicroSD storage.
- `fn get_available_space(self: & mut Self) -> Result<u64, BambuError>` - Queries the available capacity of the MicroSD card, in bytes.
- `fn disconnect(self: & mut Self)` - Sends a QUIT command and cleanly terminates the FTP session.



