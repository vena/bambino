---
paths:
  - "src/ftps/client.rs"
  - "src/client/storage.rs"
---

`FtpsClient` poisons itself after a control-channel desync. Every `write_command`/`read_response` failure, in every method — not just the `list_directory`/`upload_file`/`download_file` data-transfer window between the server's `150`/`125` reply and the matching final reply — sets `poisoned = true` (BUG-004: the six single-reply metadata/filesystem commands — `get_file_size`/`delete_file`/`create_directory`/`remove_directory`/`rename_file`/`get_available_space` — plus `negotiate_passive_port`, were missing this until this fix). A wrong-but-received reply code (e.g. RNFR getting something other than 350) is not a desync and does not poison — only a transport-level `write_command`/`read_response` error does. Every public method checks the flag first and returns `ProtocolViolation` once set — there is no un-poisoning, only reconnect via a fresh `FtpsClient::connect()`. `PrinterClient::disconnect_storage()` resets `self.ftps` to `None` so `storage()` reconnects cleanly instead of handing back a poisoned client.
