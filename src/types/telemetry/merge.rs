//! Field-by-field merging of a partial telemetry push into a cached copy.
//!
//! Firmware pushes carry only the fields that changed, so every cached telemetry struct is
//! updated by merging, never by replacement. Each [`Mergeable`] impl destructures `incoming`
//! exhaustively (`let Self { a, b, .. }` is never used), so a field added to one of these structs
//! is a compile error until its merge rule is written — a forgotten field otherwise went stale
//! silently on every partial push (#29, #43, #57).

/// A telemetry struct that can absorb a partial push of itself.
pub(crate) trait Mergeable: Clone {
    /// Folds every field `incoming` carries into `self`, keeping cached values it omits.
    fn merge_from(&mut self, incoming: &Self);
}

/// Merges a nested struct: recurse when both sides have one, take `incoming` when only it does.
pub(crate) fn merge_opt<T: Mergeable>(cached: &mut Option<T>, incoming: &Option<T>) {
    match (cached.as_mut(), incoming) {
        (Some(cached), Some(new)) => cached.merge_from(new),
        (None, Some(new)) => *cached = Some(new.clone()),
        (_, None) => {}
    }
}

/// Merges a leaf field: overwrite when `incoming` carries one, otherwise keep the cached value.
pub(crate) fn keep_new<T: Clone>(cached: &mut Option<T>, incoming: &Option<T>) {
    if incoming.is_some() {
        cached.clone_from(incoming);
    }
}

/// Merges a list keyed by `key`: entries merge into the cached entry with the same key, new keys append.
///
/// Entries the push doesn't mention are kept; callers that must prune them do so themselves.
pub(crate) fn merge_keyed<T: Mergeable, K: PartialEq>(
    cached: &mut alloc_vec::Vec<T>,
    incoming: &[T],
    key: impl Fn(&T) -> K,
) {
    for new in incoming {
        let k = key(new);
        match cached.iter_mut().find(|c| key(c) == k) {
            Some(existing) => existing.merge_from(new),
            None => cached.push(new.clone()),
        }
    }
}

#[cfg(not(feature = "std"))]
use alloc::vec as alloc_vec;
#[cfg(feature = "std")]
use std::vec as alloc_vec;
