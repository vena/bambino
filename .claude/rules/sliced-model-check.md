---
paths:
  - "src/client/files.rs"
  - "src/client/print.rs"
  - "src/ftps/project.rs"
  - "src/ftps/project/**"
  - "src/mqtt/commands/print_job.rs"
  - "src/quirks/**"
  - "src/bin/bambino-cli/storage.rs"
---

A sliced `.3mf` for another printer model must not reach a printer: its start G-code is written for that model, and an A1 file started on a P1S drove the carriage to X -48.2 with soft endstops off, into the frame (#657, `reference/03_mqtt_telemetry.md`, "Sliced-for-model guard"). The firmware checks nothing.

**One acceptance rule, in the quirks engine.** `ModelQuirks::accepts_sliced_for(printer_model_id)` is the only place that decides; never match on `PrinterModel` or compare ids elsewhere. A row's `sliced_for` holds its own BambuStudio printer type ids plus the `compatible_machine` list from `resources/printers/<id>.json` (`00.00.00.00`). A new model's row gets both, and the `UNKNOWN` row accepts nothing.

**The check runs in three places, all through `accepts_sliced_for`:** `upload_file` and `start_print` share `refuse_unaccepted_model` (`src/client/print.rs`), and the CLI builds its own refusal. `upload_file` (before any transfer; `ModelCheck::Skip` overrides), `start_print` (`PrintJobConfig::sliced_for`; `model_unchecked()` overrides), and the CLI's `files ... map --print` (`--allow-other-model` sets both overrides). A file with no `printer_model_id` is refused like a mismatch.

**This is BambuStudio's blocking rule**, not a warning: `SelectMachineDialog::is_blocking_printing` blocks a send whose file model differs from the printer's and isn't in its compatible list, for a fresh slice and for an on-printer file (whose `printer_model_id` Studio fetches through the file tunnel). bambino refuses where Studio blocks.

**`SlicedFor::NotSupplied` (the default) is refused too**; the caller supplies the id or calls `model_unchecked()`. bambino can't fetch an on-printer file's `slice_info.config` until the file tunnel arm lands (#636), and over FTPS only by a full download (no `REST`). Don't read the file inside `start_print` to close that; when the tunnel arm lands, that is where the on-printer read belongs.

The id comes from the plate's `printer_model_id` metadata (`read_sliced_for`); two plates that disagree are `ProtocolViolation`, an unsliced project is `InvalidArgument`. `PrinterFile::sliced_for` is `None` from a listing.
