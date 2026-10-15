---
paths:
  - "src/client/job.rs"
  - "src/client/telemetry.rs"
  - "src/ftps/project.rs"
  - "src/ftps/project/**"
---

A running job's plate number is resolved through a chain (`src/client/job.rs` module doc): `plate_idx`, then the printer's echo of the `project_file` start command for the running `gcode_file`, then the job's `.3mf` holding G-code for exactly one plate (`ftps::project::gcode_plates`). **When no link answers, the plate is `None`. Never default to 1 and never parse a number out of the file name.** BambuStudio's skip-objects dialog and bambuddy both do one or the other, and a P1S job shows why it fails: a sent `.3mf` carries every plate's images under their own numbers, so plate 1's images are present and look right while plate 2 prints (`reference/03_mqtt_telemetry.md`, "P1 and A1 printers don't send `plate_idx`").

The cached inputs must never describe a different job. `plate_idx` is dropped when `gcode_file` changes. A recorded start applies only to the `gcode_file` it named, only on the connection it was seen on, and only until that job ends. Loosening any of these trades a `None` for a possible wrong plate.

New sources (the port-6000 file service, GitHub issue #633) join the chain as a `PlateSource` variant. They are gated per model through a `ModelQuirks` method, never a model match.
