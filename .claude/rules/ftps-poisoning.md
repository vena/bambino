---
paths:
  - "src/ftps/client.rs"
  - "src/client/storage.rs"
---

`BambuFtpsClient` poisons itself after a control-channel desync. `list_directory`/`upload_file`/`download_file` set a `poisoned` flag if an error occurs between the server's `150`/`125` reply and the matching final reply, since that window leaves the control channel mid-response. Every public method checks it first and returns `ProtocolViolation` once set — there is no un-poisoning, only reconnect via a fresh `BambuFtpsClient::connect()`. `PrinterClient::disconnect_storage()` resets `self.ftps` to `None` so `storage()` reconnects cleanly instead of handing back a poisoned client.
