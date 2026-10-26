//! The caller-supplied byte source an upload reads its file from, a chunk at a time.
//!
//! A `.3mf` can be tens of MB, more than an embedded consumer can hold in RAM. An upload takes an
//! [`UploadSource`] instead of one slice, so the file can be read from an SD card as it goes out.

#[cfg(not(feature = "std"))]
use alloc::format;

use crate::error::Error;

/// A file's bytes, read by offset, for an upload.
///
/// The read is positional because the file tunnel's init reply can ask to resume from an offset,
/// and the whole file's MD5 goes on the last chunk, so the bytes before that offset are read
/// too. It is async so an SD-card read doesn't block an embassy executor.
///
/// Implemented for `&[u8]` and for `&mut S` of any source, so an upload takes a source by value:
/// a slice directly, or a file as `&mut file`.
#[allow(async_fn_in_trait)]
pub trait UploadSource {
    /// Total size in bytes, known before the upload starts.
    fn len(&self) -> u64;

    /// Whether the source holds no bytes.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Reads up to `buf.len()` bytes starting at `offset` into `buf`, and returns how many.
    ///
    /// Returning 0 for an `offset` before [`len`](Self::len) is a short source, which fails the
    /// upload.
    ///
    /// # Errors
    ///
    /// Whatever the source's own read fails with.
    async fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, Error>;
}

impl UploadSource for &[u8] {
    fn len(&self) -> u64 {
        <[u8]>::len(self) as u64
    }

    async fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, Error> {
        let Ok(start) = usize::try_from(offset) else {
            return Ok(0);
        };
        let Some(rest) = self.get(start..) else {
            return Ok(0);
        };
        let n = rest.len().min(buf.len());
        buf[..n].copy_from_slice(&rest[..n]);
        Ok(n)
    }
}

impl<S: UploadSource + ?Sized> UploadSource for &mut S {
    fn len(&self) -> u64 {
        (**self).len()
    }

    fn is_empty(&self) -> bool {
        (**self).is_empty()
    }

    async fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, Error> {
        (**self).read_at(offset, buf).await
    }
}

/// Reads the next piece of `source` at `offset` into `buf`, at most `buf.len()` and never past `source.len()`.
///
/// A source that returns 0 before its length is short, which is [`Error::InvalidArgument`]
/// naming the offset.
pub(crate) async fn read_next<S: UploadSource>(
    source: &mut S,
    offset: u64,
    buf: &mut [u8],
) -> Result<usize, Error> {
    let remaining = source.len().saturating_sub(offset);
    let want = usize::try_from(remaining).map_or(buf.len(), |r| r.min(buf.len()));
    let n = source.read_at(offset, &mut buf[..want]).await?;
    if n == 0 && want > 0 {
        return Err(Error::InvalidArgument(
            format!(
                "upload source ended at offset {offset}, short of its length of {}",
                source.len()
            )
            .into(),
        ));
    }
    if n > want {
        return Err(Error::InvalidArgument(
            format!("upload source returned more bytes than asked for at offset {offset}").into(),
        ));
    }
    Ok(n)
}
