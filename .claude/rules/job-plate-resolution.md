---
paths:
  - "src/client/job.rs"
  - "src/client/files.rs"
  - "src/client/telemetry.rs"
  - "src/ftps/project.rs"
  - "src/ftps/project/**"
---

A running job's plate number is resolved through a chain (`src/client/job.rs` module doc): `plate_idx`, then the printer's echo of the `project_file` start command for the running `gcode_file`, then the job's `.3mf` holding G-code for exactly one plate (`ftps::project::gcode_plates`). **When no link answers, the plate is `None`. Never default to 1 and never parse a number out of the file name.** BambuStudio's skip-objects dialog and bambuddy both do one or the other, and a P1S job shows why it fails: a sent `.3mf` carries every plate's images under their own numbers, so plate 1's images are present and look right while plate 2 prints (`reference/03_mqtt_telemetry.md`, "P1 and A1 printers don't send `plate_idx`").

The cached inputs must never describe a different job. `plate_idx` is dropped when `gcode_file` changes. A recorded start applies only to the `gcode_file` it named, only on the connection it was seen on, and only until that job ends. Loosening any of these trades a `None` for a possible wrong plate.

`job_file` follows the same rule for the file: the echoed `url`, else `/` + a bare `.3mf` `gcode_file`, confirmed by `SIZE`, else `None`. A screen start reports no folder (`reference/02_ftps.md` [REF-FTPS-OPS]). The card is deliberately not searched for the name: that costs one `LIST` per folder and only helps hand-placed files (owner decision, 2026-10-15).

The running job's plate applies only to the running job's file. `read_plate_images` never consults `job_plate()`: given no plate, it takes one from the file's own G-code list or returns none. Only `read_running_job_images` passes `job_plate()`'s plate, and only to `job_file()`'s file.

New plate sources join the chain as a `PlateSource` variant, gated per model through a `ModelQuirks` method, never a model match. The port-6000 file tunnel is not a plate source: `read_running_job_images` reads one plate's members through it by name, so it needs the plate from `job_plate()` and falls back to the FTPS route without one. Its gate is `Capabilities::file_tunnel_support()` (`fun` bit 31).
