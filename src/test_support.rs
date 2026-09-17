//! Platform-agnostic test doubles shared across the crate's `#[cfg(test)]` modules.
//!
//! Every type here compiles under `no_std` + `alloc`, so a unit test written against it runs
//! under *every* feature set — including `alloc`/`embassy`, where no tokio backend exists. That
//! is the whole point: `pub(crate)` no_std code can only be reached from a `#[cfg(test)]` module
//! inside the lib, never from an integration test, so a test double that drags in `crate::io::tokio`
//! makes the entire no_std surface untestable (#291).
//!
//! Use `crate::io::tokio`'s real types only in a test that is *about* the tokio backend, or that
//! genuinely asserts wall-clock behaviour; gate that test on `feature = "tokio"`.

#[cfg(not(feature = "std"))]
use alloc::collections::VecDeque;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use core::cell::Cell;
#[cfg(feature = "std")]
use std::collections::VecDeque;

use crate::io::{TimerError, TimerProvider};

/// A [`TimerProvider`] driving a virtual monotonic clock instead of wall time.
///
/// `sleep()` returns immediately and advances the clock by the requested duration, so a loop
/// that paces itself with `sleep()` and terminates on `now_millis()` (`discover_devices_with`'s
/// listen loop, for one) completes instantly and deterministically rather than burning the real
/// timeout. `advance()` moves the clock directly for tests that never sleep.
///
/// Uses [`Cell`] rather than a mutex so it works under `no_std`; consequently it is `!Sync` and
/// must stay on a single task. That matches how the crate's tests use it — one timer borrowed by
/// one future.
///
/// **Don't use this for code that races an I/O read against `sleep()`** (`poll_wire`,
/// `read_exact_packet`): `sleep()` completes instantly in real time, so the race resolves to
/// "timed out" on essentially every call. That is the same hazard
/// [`TimerProvider::has_real_clock`] exists for — such a test needs a real platform timer.
pub(crate) struct MockTimer {
    clock: Cell<u64>,
}

impl MockTimer {
    pub(crate) fn new() -> Self {
        Self {
            clock: Cell::new(0),
        }
    }

    /// Moves the virtual clock forward by `ms`.
    pub(crate) fn advance(&self, ms: u64) {
        self.clock.set(self.clock.get().saturating_add(ms));
    }
}

impl TimerProvider for MockTimer {
    async fn sleep(&self, duration: core::time::Duration) -> Result<(), TimerError> {
        self.advance(duration.as_millis() as u64);
        Ok(())
    }

    fn now_millis(&self) -> u64 {
        self.clock.get()
    }
}

/// What a [`MockIo`] delivers once its queued read chunks run out.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadExhausted {
    /// Report EOF (`Ok(0)`) forever. The default.
    Eof,
    /// Keep returning a full buffer of zero bytes forever, never signalling EOF — for driving a
    /// reader's size cap against a stream that never stops sending.
    Infinite,
}

/// An in-memory [`AsyncIo`](crate::io::AsyncIo) stream with a scripted read side and a recording
/// write side.
///
/// Each queued chunk is returned by exactly one `read()` call (truncated to the caller's buffer,
/// with the remainder re-queued), so a test can control precisely how a payload is split across
/// socket reads — the property partial-line and partial-frame reassembly logic turns on. Each
/// `write()` call is recorded as its own entry in [`writes`](Self::writes), so a test can assert
/// how many separate writes a function issued rather than only the concatenated bytes.
pub(crate) struct MockIo {
    reads: VecDeque<Vec<u8>>,
    exhausted: ReadExhausted,
    /// One entry per `write()` call, in order.
    pub(crate) writes: Vec<Vec<u8>>,
}

impl MockIo {
    /// A stream whose reads replay `chunks` in order, one chunk per `read()` call, then report EOF.
    pub(crate) fn with_chunks(chunks: &[&[u8]]) -> Self {
        Self {
            reads: chunks.iter().map(|c| c.to_vec()).collect(),
            exhausted: ReadExhausted::Eof,
            writes: Vec::new(),
        }
    }

    /// A write-only stream: reads report EOF immediately.
    pub(crate) fn empty() -> Self {
        Self::with_chunks(&[])
    }

    /// A stream that returns a full buffer of zero bytes on every read, forever, and never
    /// signals EOF.
    pub(crate) fn infinite() -> Self {
        Self {
            reads: VecDeque::new(),
            exhausted: ReadExhausted::Infinite,
            writes: Vec::new(),
        }
    }

    /// Every byte written so far, concatenated across all `write()` calls.
    pub(crate) fn written(&self) -> Vec<u8> {
        self.writes.iter().flatten().copied().collect()
    }
}

impl embedded_io_async::ErrorType for MockIo {
    type Error = embedded_io_async::ErrorKind;
}

impl embedded_io_async::Read for MockIo {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let Some(mut chunk) = self.reads.pop_front() else {
            return match self.exhausted {
                ReadExhausted::Eof => Ok(0),
                // `buf` is never zero-length in this crate's callers, so this always makes
                // progress and can't stall a read loop.
                ReadExhausted::Infinite => {
                    buf.fill(0);
                    Ok(buf.len())
                }
            };
        };
        let n = chunk.len().min(buf.len());
        buf[..n].copy_from_slice(&chunk[..n]);
        if n < chunk.len() {
            // The caller's buffer couldn't take the whole chunk — the rest stays at the head of
            // the queue for the next read rather than being dropped.
            chunk.drain(..n);
            self.reads.push_front(chunk);
        }
        Ok(n)
    }
}

impl embedded_io_async::Write for MockIo {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.writes.push(buf.to_vec());
        Ok(buf.len())
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_io_async::{Read, Write};

    #[tokio::test]
    async fn test_mock_timer_now_millis_controllable() {
        let timer = MockTimer::new();
        assert_eq!(timer.now_millis(), 0);
        timer.advance(250);
        assert_eq!(timer.now_millis(), 250);
        timer.advance(750);
        assert_eq!(timer.now_millis(), 1000);
    }

    #[tokio::test]
    async fn test_mock_timer_sleep_advances_the_virtual_clock() {
        // The property that lets a `sleep()`-paced timeout loop terminate instantly instead of
        // spinning forever on a clock that never moves.
        let timer = MockTimer::new();
        timer
            .sleep(core::time::Duration::from_millis(500))
            .await
            .unwrap();
        assert_eq!(timer.now_millis(), 500);
    }

    #[tokio::test]
    async fn test_mock_io_returns_one_chunk_per_read_then_eof() {
        let mut io = MockIo::with_chunks(&[b"abc", b"de"]);
        let mut buf = [0u8; 16];

        assert_eq!(io.read(&mut buf).await.unwrap(), 3);
        assert_eq!(&buf[..3], b"abc");
        assert_eq!(io.read(&mut buf).await.unwrap(), 2);
        assert_eq!(&buf[..2], b"de");
        assert_eq!(io.read(&mut buf).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_mock_io_requeues_the_remainder_of_an_oversized_chunk() {
        let mut io = MockIo::with_chunks(&[b"abcdef"]);
        let mut buf = [0u8; 4];

        assert_eq!(io.read(&mut buf).await.unwrap(), 4);
        assert_eq!(&buf, b"abcd");
        assert_eq!(io.read(&mut buf).await.unwrap(), 2);
        assert_eq!(&buf[..2], b"ef");
    }

    #[tokio::test]
    async fn test_mock_io_infinite_never_reports_eof() {
        let mut io = MockIo::infinite();
        let mut buf = [0u8; 8];
        for _ in 0..3 {
            assert_eq!(io.read(&mut buf).await.unwrap(), 8);
        }
    }

    #[tokio::test]
    async fn test_mock_io_records_each_write_separately() {
        let mut io = MockIo::empty();
        io.write(b"one").await.unwrap();
        io.write(b"two").await.unwrap();

        assert_eq!(io.writes.len(), 2);
        assert_eq!(io.writes[0], b"one");
        assert_eq!(io.written(), b"onetwo");
    }
}
