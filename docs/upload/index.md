*[bambino](../index.md) / [upload](index.md)*

---

# Module `upload`

The caller-supplied byte source an upload reads its file from, a chunk at a time.

A `.3mf` can be tens of MB, more than an embedded consumer can hold in RAM. An upload takes an
[`UploadSource`](#uploadsource) instead of one slice, so the file can be read from an SD card as it goes out.

## Quick Reference

| Item | Kind | Description |
|------|------|-------------|
| [`UploadSource`](#uploadsource) | trait | A file's bytes, read by offset, for an upload. |

## Traits

### `UploadSource`

```rust
trait UploadSource { ... }
```


A file's bytes, read by offset, for an upload.

The read is positional because the file tunnel's init reply can ask to resume from an offset,
and the whole file's MD5 goes on the last chunk, so the bytes before that offset are read
too. It is async so an SD-card read doesn't block an embassy executor.

Implemented for `&[u8]` and for `&mut S` of any source, so an upload takes a source by value:
a slice directly, or a file as `&mut file`.

#### Required Methods

- `fn len(&self) -> u64`

  Total size in bytes, known before the upload starts.

- `fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, Error>`

  Reads up to `buf.len()` bytes starting at `offset` into `buf`, and returns how many.

  Returning 0 for an `offset` before [`len`](#uploadsource) is a short source, which fails the
  upload.

  # Errors

  Whatever the source's own read fails with.

#### Provided Methods 

- `fn is_empty(&self) -> bool`

  Whether the source holds no bytes.

#### Implementors

- `&[u8]`
- `&mut S`

