//! The one tolerant reader behind every telemetry field firmware sends with an inconsistent JSON type.
//!
//! **Error policy, for every deserializer here:** a value of the wrong shape degrades *that field*
//! (to `None`, or to the documented default for a required field) and never fails the frame. A
//! telemetry push carries dozens of fields; losing all of them because one arrived as `"2"` instead
//! of `2`, or as an object nobody expected, costs far more than the one value this crate couldn't
//! read. The only exception is [`deserialize_permissive_string`], whose field has no "not
//! reported" state to degrade to.

#[cfg(not(feature = "std"))]
use alloc::string::{String, ToString};

use serde::{Deserialize, Deserializer};

/// Any JSON value, read without committing to a type.
#[derive(Deserialize)]
#[serde(untagged)]
enum Loose {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    /// An object or array — a shape none of these fields has, kept only so it degrades.
    Other(serde::de::IgnoredAny),
}

impl Loose {
    fn read<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // `Other` accepts anything, so this only fails on a malformed document, which serde_json
        // would have rejected before reaching the field.
        Loose::deserialize(deserializer)
    }

    fn into_bool(self) -> Option<bool> {
        match self {
            Loose::Bool(b) => Some(b),
            Loose::Int(i) => Some(i != 0),
            Loose::Str(s) => {
                let s = s.trim();
                Some(
                    s.eq_ignore_ascii_case("HAS_SDCARD_NORMAL")
                        || s.eq_ignore_ascii_case("true")
                        || s == "1",
                )
            }
            Loose::Null | Loose::Float(_) | Loose::Other(_) => None,
        }
    }

    fn into_i64(self) -> Option<i64> {
        match self {
            Loose::Int(i) => Some(i),
            Loose::Float(f) if f.is_finite() => Some(f as i64),
            Loose::Str(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    fn into_f64(self) -> Option<f64> {
        match self {
            Loose::Int(i) => Some(i as f64),
            Loose::Float(f) => Some(f),
            Loose::Str(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    fn into_string(self) -> Option<String> {
        match self {
            Loose::Str(s) => Some(s),
            Loose::Int(i) => Some(i.to_string()),
            Loose::Float(f) => Some(f.to_string()),
            Loose::Null | Loose::Bool(_) | Loose::Other(_) => None,
        }
    }
}

/// A bool sent as a JSON bool, an integer (non-zero is true) or a string (`"true"`, `"1"`, or the `sdcard` field's `"HAS_SDCARD_NORMAL"`).
pub(crate) fn deserialize_permissive_opt_bool<'de, D>(
    deserializer: D,
) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Loose::read(deserializer)?.into_bool())
}

/// An integer sent as a JSON number or a decimal string; `None` when it doesn't fit `T`.
///
/// Needed where BambuStudio itself branches on the wire type rather than assuming one, and
/// applied to every numeric telemetry field rather than only the confirmed ones: it costs
/// nothing on well-formed input, and a plain `Option<i32>` fails the whole frame on the quoted
/// form.
pub(crate) fn deserialize_permissive_opt_int<'de, D, T>(
    deserializer: D,
) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: TryFrom<i64>,
{
    Ok(Loose::read(deserializer)?
        .into_i64()
        .and_then(|i| T::try_from(i).ok()))
}

/// A float sent as a JSON number (integer or not) or a decimal string.
///
/// Temperatures arrive as integers on some models and floats on others [REF-THER-DECODE]; the
/// quoted form is tolerated for the same reason as [`deserialize_permissive_opt_int`].
pub(crate) fn deserialize_permissive_opt_f64<'de, D>(
    deserializer: D,
) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Loose::read(deserializer)?.into_f64())
}

/// A 32-bit flag word, keeping its bit pattern when firmware sends it as a negative number.
///
/// `home_flag` and `flag3` are signed 32-bit ints on the wire [REF-HOMEFLAG]; bit 31 set
/// produces a negative JSON number. This masks it into `u32` the way [REF-HOMEFLAG]'s documented
/// `flag & 0xFFFFFFFF` does, instead of rejecting it.
pub(crate) fn deserialize_permissive_opt_flags<'de, D>(
    deserializer: D,
) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Loose::read(deserializer)?.into_i64().map(|i| i as u32))
}

/// A string sent as a JSON string or a bare number, rendered back to its decimal text.
///
/// `mc_print_stage` is parsed with both an `is_string()` and an `is_number()` arm in
/// BambuStudio's `DeviceManager.cpp:3071-3076`; binding such a field as a plain
/// `Option<String>` would fail the whole frame on the numeric form.
pub(crate) fn deserialize_permissive_opt_string<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Loose::read(deserializer)?.into_string())
}

/// Required-field counterpart of [`deserialize_permissive_opt_string`].
///
/// The one deserializer here that fails rather than degrading: these fields have no "not
/// reported" state, and inventing an empty string would hand the caller a value the printer
/// never sent.
pub(crate) fn deserialize_permissive_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error as _;
    Loose::read(deserializer)?
        .into_string()
        .ok_or_else(|| D::Error::custom("expected a string or number"))
}

/// An `HmsEntry.attr`/`.code` word: a plain integer or a `0x`/`0X`-prefixed hex string.
///
/// BambuStudio's `ParseHMSItems` (`DevHMS.cpp:42-61`) pushes a default-zeroed item on a
/// malformed entry rather than aborting the whole message; bambuddy (`bambu_mqtt.py:2756-2761`)
/// additionally tolerates hex-string values. Any other shape, including an unprefixed decimal
/// string, reads `0`.
pub(crate) fn deserialize_permissive_hms_u32<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match Loose::read(deserializer)? {
        Loose::Int(i) => u32::try_from(i).unwrap_or(0),
        Loose::Str(s)
            if s.trim_start()
                .get(..2)
                .is_some_and(|p| p.eq_ignore_ascii_case("0x")) =>
        {
            super::bits::hex_u64(&s)
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(0)
        }
        _ => 0,
    })
}
