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

**`SlicedFor::NotSupplied` (the default) is looked up through the file tunnel, as Studio does**, in `start_print`'s `sliced_for_from_printer`. Only the tunnel: over FTPS an on-printer file's `slice_info.config` needs a full download (no `REST`), so don't read it there. A printer whose `file_tunnel_support` isn't supported (P1/A1) is refused, as is any failure of the lookup (`Error::ModelMismatch` naming the cause and both ways forward, `PrintJobConfig::sliced_for` and `model_unchecked()`); nothing is published. The lookup lists `FileKind::Model` (no storage label for `JobFileLocation::Card`, `emmc` for `InternalStorage`, `reference/08_file_service.md` `REQUEST_MEDIA_ABILITY`), matches `job_filename`'s final component against `name` (several matches are refused), then `read_file_member` (`SUB_FILE` sends no `zip`, so the reply is the raw member, `reference/08_file_service.md` `SUB_FILE`) of `Metadata/slice_info.config`, parsed by `parse_sliced_for`. **Unverified on hardware (#637).** The tunnel client's other `PrinterClient` uses are #636.

The id comes from the plate's `printer_model_id` metadata (`read_sliced_for`); two plates that disagree are `ProtocolViolation`, an unsliced project is `InvalidArgument`. `PrinterFile::sliced_for` is `None` from a listing.
