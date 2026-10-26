*[bambino](../../index.md) / [ftps](../index.md) / [project](index.md)*

---

# Module `project`

# Reading plate images from the front of a job's `.3mf`

A sliced `.3mf` is a ZIP archive. Bambu Studio and OrcaSlicer both write it in a fixed order
(`_BBS_3MF_Exporter::_save_model_to_file` in each): `[Content_Types].xml`, then every plate's
thumbnails (`Metadata/plate_N.png`, `plate_N_small.png`, `plate_no_light_N.png`, `top_N.png`,
`pick_N.png`), stored uncompressed, then `Metadata/plate_N.json`, and only then the model,
G-code and configs. So the plate and pick images sit in the first few tens of KB of a file
that can be tens of MB, and [`front_members`](#front-members) reads them from a prefix of the file
([`FtpsClient::read_front_members`](../index.md) fetches one).

Every entry these slicers write sets general-purpose flag bit 3 (sizes in a data descriptor
after the data, not in the local header), so the walk can't skip an entry by its header. It
finds a stored PNG's end after its `IEND` chunk, and a deflated entry's end by the data
descriptor that follows it (its compressed size equals the distance walked, and a ZIP header
follows). The front walk never inflates. See `reference/02_ftps.md` [REF-FTPS-3MF-FRONT].

The one thing here that does inflate is [`read_plate_filaments`](#read-plate-filaments), which reads a plate's
filament list from `Metadata/slice_info.config`: that member sits near the end of the file,
deflated, so it is found through the ZIP's central directory (see the `slice_info` module) from
a file the caller holds, not from the front prefix.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`JobPrefix`](#jobprefix) | struct | The start of a job's `.3mf` and the plates it holds G-code for, from [`FtpsClient::read_job_prefix`](../index.md). |
| [`front_members`](#front-members) | fn | Finds the members named in `names` in `prefix`, the first bytes of a `.3mf`, returning each one's bytes or `None`. |
| [`gcode_plates`](#gcode-plates) | fn | Returns the plates whose G-code a `.3mf` holds, read from the start of the file, in archive order. |
| [`pick_identify_id`](#pick-identify-id) | fn | Decodes one pick-image pixel, given as `[r, g, b, a]`, into the `identify_id` of the object it belongs to. |
| [`pick_image_member`](#pick-image-member) | fn | Name of plate `plate`'s pick image: `Metadata/pick_<plate>.png`. |
| [`plate_image_member`](#plate-image-member) | fn | Name of plate `plate`'s image inside the `.3mf`: `Metadata/plate_<plate>.png`. |
| [`plate_small_image_member`](#plate-small-image-member) | fn | Name of plate `plate`'s 128×128 thumbnail: `Metadata/plate_<plate>_small.png`. |

## Types

### `PlateFilament`

```rust
struct PlateFilament {
    pub id: u32,
    pub filament_type: String,
    pub color: String,
    pub tray_info_idx: String,
}
```

One filament a plate uses, as `slice_info.config` lists it.

#### Fields

- **`id`**: `u32`

  The project filament index, **1-based**, as the file writes it.
  
  This is the `filament_id` `build_ams_mapping2` takes, and
  the position (minus one) of this filament's entry in an `ams_mapping2` array. BambuStudio
  holds it 0-based internally and writes `id + 1`.

- **`filament_type`**: `String`

  The material, such as `"PLA"` or `"PETG"`.

- **`color`**: `String`

  The colour as the file writes it, `"#RRGGBB"` (or `"#RRGGBBAA"`).

- **`tray_info_idx`**: `String`

  The filament preset's id, such as `"GFA00"`; empty for a custom preset.
  
  BambuStudio's `filament_id`, which it matches against a tray's `tray_info_idx`. The file
  has no `setting_id`, so BambuStudio's finer preset match never fires for a file's filament.

#### Trait Implementations

##### `impl Clone for PlateFilament`

- <span id="platefilament-clone"></span>`fn clone(&self) -> PlateFilament` — [`PlateFilament`](#platefilament)

##### `impl Debug for PlateFilament`

- <span id="platefilament-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for PlateFilament`

##### `impl PartialEq for PlateFilament`

- <span id="platefilament-partialeq-eq"></span>`fn eq(&self, other: &PlateFilament) -> bool` — [`PlateFilament`](#platefilament)

### `PlateFilaments`

```rust
struct PlateFilaments {
    pub plate: u32,
    pub filaments: Vec<PlateFilament>,
    pub filament_maps: Option<Vec<u8>>,
}
```

The filaments one plate uses, and the extruder each project filament was assigned to.

#### Fields

- **`plate`**: `u32`

  The plate, 1-based, as telemetry's `plate_idx` reports it.

- **`filaments`**: `Vec<PlateFilament>`

  The filaments the plate prints with, in file order. A project filament the plate doesn't
  use isn't listed.

- **`filament_maps`**: `Option<Vec<u8>>`

  The extruder of every **project** filament, indexed by `id - 1`: `1` is the left (deputy)
  extruder and `2` the right (main) one. `None` when the file records none.
  
  Only a dual-nozzle project sets it to anything but a single extruder.

#### Implementations

- <span id="platefilaments-extruder-of"></span>`fn extruder_of(&self, id: u32) -> Option<u8>`

  The extruder value of filament `id` (1-based), or `None` when the file records none for it.

- <span id="platefilaments-is-deputy"></span>`fn is_deputy(&self, id: u32) -> bool`

  Whether filament `id` (1-based) is assigned to the left (deputy) extruder.

#### Trait Implementations

##### `impl Clone for PlateFilaments`

- <span id="platefilaments-clone"></span>`fn clone(&self) -> PlateFilaments` — [`PlateFilaments`](#platefilaments)

##### `impl Debug for PlateFilaments`

- <span id="platefilaments-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`

##### `impl Eq for PlateFilaments`

##### `impl PartialEq for PlateFilaments`

- <span id="platefilaments-partialeq-eq"></span>`fn eq(&self, other: &PlateFilaments) -> bool` — [`PlateFilaments`](#platefilaments)

### `JobPrefix<'a>`

```rust
struct JobPrefix<'a> {
    pub bytes: &'a [u8],
    pub gcode_plates: Vec<u32>,
}
```

The start of a job's `.3mf` and the plates it holds G-code for, from [`FtpsClient::read_job_prefix`](../index.md).

#### Fields

- **`bytes`**: `&'a [u8]`

  The bytes read, from offset 0. Pass them to [`front_members`](#front-members) for plate images.

- **`gcode_plates`**: `Vec<u32>`

  What [`gcode_plates`](#gcode-plates) returns for these bytes.

#### Trait Implementations

##### `impl Debug for JobPrefix<'a>`

- <span id="jobprefix-debug-fmt"></span>`fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result`


---

## Functions

### `parse_slice_info`

```rust
fn parse_slice_info(text: &str, plate: u32) -> Result<PlateFilaments, crate::error::Error>
```

**Types:** [`PlateFilaments`](#platefilaments), [`Error`](../../error/index.md#error)

Parses plate `plate`'s filaments out of an already inflated `slice_info.config`.

Reads the `<plate>` blocks BambuStudio's exporter writes: `<metadata key="index" value="N"/>`
names the plate, `<metadata key="filament_maps" value="1 2 ..."/>` gives the extruder of each
project filament, and each `<filament id=".." tray_info_idx=".." type=".." color=".."/>` is a
filament the plate uses. `<mixed_filament>` elements are not filaments here. The XML entities
`&amp; &lt; &gt; &quot; &apos;` are unescaped in attribute values; any other `&` is kept.

# Errors

- [`Error::InvalidArgument`](../../error/index.md#error) when no plate has index `plate`.
- [`Error::ProtocolViolation`](../../error/index.md#error) for markup it can't scan, a `<filament>` with no valid positive
  `id`, or a `filament_maps` with a value that isn't a number.

### `read_plate_filaments`

```rust
async fn read_plate_filaments(source: impl UploadSource, plate: u32) -> Result<PlateFilaments, crate::error::Error>
```

**Types:** [`UploadSource`](../../upload/index.md#uploadsource), [`PlateFilaments`](#platefilaments), [`Error`](../../error/index.md#error)

Reads plate `plate`'s filaments from the `.3mf` in `source`, found through the ZIP's central directory.

`plate` is 1-based. Reads the file's last 4 KiB (64 KiB only for an archive with a long
comment), the central directory in 4 KiB pieces, and then the one member, inflating it with
a cap of 2 MiB.

# Errors

- [`Error::InvalidArgument`](../../error/index.md#error) when `slice_info.config` has no plate `plate`, or `source` fails
  its own length.
- [`Error::ProtocolViolation`](../../error/index.md#error) when the bytes aren't a ZIP this reader follows: no end of
  central directory, a Zip64 or multi-disk archive, an encrypted or oversized member, a
  method other than stored or deflate, a failed inflate or CRC-32, or no
  `Metadata/slice_info.config` at all, or XML it can't read.

Whatever `source`'s own read fails with.

### `front_members`

```rust
fn front_members<'a, const N: usize>(prefix: &'a [u8], names: [&str; N]) -> Result<[Option<&'a [u8]>; N], crate::error::Error>
```

**Types:** [`Error`](../../error/index.md#error)

Finds the members named in `names` in `prefix`, the first bytes of a `.3mf`, returning each one's bytes or `None`.

`None` means the member isn't in the front section (`[Content_Types].xml` and the
`Metadata/` entries that follow it): a plate number the job doesn't have, or a slicer that
wrote no pick image. Members are returned only when stored uncompressed, as the slicers write
PNGs; a requested member that is compressed is an error rather than bytes the caller can't
use. Stored members are checked against their data descriptor's CRC-32 and sizes.

The walk stops once every requested member is found, so `prefix` needs to reach only the end
of the last of them.

# Errors

- [`Error::InvalidArgument`](../../error/index.md#error) when `prefix` ends before the walk could settle every name:
  read a longer prefix.
- [`Error::ProtocolViolation`](../../error/index.md#error) when the bytes aren't the layout described in the module
  doc: not a ZIP, an encrypted entry, an entry whose end can't be found without inflating
  it, a descriptor that disagrees with the data, or a requested member that is compressed.

### `gcode_plates`

```rust
fn gcode_plates(prefix: &[u8]) -> Result<Vec<u32>, crate::error::Error>
```

**Types:** [`Error`](../../error/index.md#error)

Returns the plates whose G-code a `.3mf` holds, read from the start of the file, in archive order.

Both slicers' exporters (`_BBS_3MF_Exporter::_save_model_to_file`) write one
`Metadata/plate_<N>.gcode.md5` for every plate with G-code, all in one loop and before the
first `Metadata/plate_<N>.gcode`. A file sent to a printer stores no meshes
(`SaveStrategy::SkipModel`), so that list sits just past the front section: within the first
30–55 KB on the files measured (`reference/02_ftps.md` [REF-FTPS-3MF-FRONT]). The printer can
only print G-code the file holds, so when this returns exactly one plate, that is the plate
being printed. Several mean the file alone can't say which.

The walk ends at the first plate's G-code member, or at the central directory.

# Errors

As [`front_members`](#front-members): [`Error::InvalidArgument`](../../error/index.md#error) when `prefix` ends before the list does,
[`Error::ProtocolViolation`](../../error/index.md#error) for a file the walk can't follow, such as one that stores its
meshes uncompressed before the list.

### `pick_identify_id`

```rust
const fn pick_identify_id(rgba: [u8; 4]) -> Option<u32>
```

Decodes one pick-image pixel, given as `[r, g, b, a]`, into the `identify_id` of the object it belongs to.

The id is `(b << 16) | (g << 8) | r`, as BambuStudio's `SkipPartCanvas::LoadPickImage` and
`GetIdAtImagePt` (via `SkipIdHelper::reverse`) and ha-bambulab's
`_identify_objects_in_pick_image` decode it. Returns `None` for background: a transparent
pixel (ha-bambulab skips alpha 0) or black (BambuStudio drops alpha and treats id 0 as none).

Anti-aliased edges hold blended colours that decode to ids no object has. BambuStudio takes
each object's majority colour; count pixels per id and keep the dominant ones, or check the
ids against telemetry, rather than trusting every decoded pixel. A plate whose pick image
holds one id has one object, and skipping objects on it is pointless.

### `pick_image_member`

```rust
fn pick_image_member(plate: u32) -> String
```

Name of plate `plate`'s pick image: `Metadata/pick_<plate>.png`.

Each object on the plate is filled with one flat colour encoding its `identify_id`, the id
[`PrinterClient::skip_objects`](../../client/index.md#printerclient) takes; decode a
pixel with [`pick_identify_id`](#pick-identify-id).

### `plate_image_member`

```rust
fn plate_image_member(plate: u32) -> String
```

Name of plate `plate`'s image inside the `.3mf`: `Metadata/plate_<plate>.png`.

`plate` is 1-based, as telemetry's `plate_idx` reports it; `plate_idx` is authoritative over
the archive's own plate data (`reference/03_mqtt_telemetry.md`, "`plate_idx`"). P1 and A1
printers don't send `plate_idx`; see that section for what BambuStudio falls back to.

### `plate_small_image_member`

```rust
fn plate_small_image_member(plate: u32) -> String
```

Name of plate `plate`'s 128×128 thumbnail: `Metadata/plate_<plate>_small.png`.

