//! Full truth table for `supports_ams_remote_drying`, asserted against every model.
//!
//! The capability model was rewritten three times (#238, #240, #241) and the intermediate
//! shapes each left doc comments describing behavior the code no longer had. This pins the
//! resolved answer for every `(model, fun2, firmware)` combination that matters, so the
//! description and the behavior cannot drift apart again without a test failing.

use bambino::models::PrinterModel;
use bambino::quirks::QuirkContext;

/// Every model, with nothing reported — the path that actually runs on real hardware, since
/// `fun2` is BambuStudio-only and P1/A1 send no capability bitfields at all.
#[test]
fn test_nothing_reported() {
    let ctx = QuirkContext::empty();
    let cases = [
        // Never: no AMS 2 Pro / AMS-HT compatibility.
        (PrinterModel::A1, false),
        (PrinterModel::A1Mini, false),
        // Never: screen-only.
        (PrinterModel::P1P, false),
        (PrinterModel::P1S, false),
        // Firmware-gated, but an unread version falls back to the model answer rather than
        // denying — "nobody asked" is not "too old".
        (PrinterModel::X1C, true),
        (PrinterModel::P2S, true),
        (PrinterModel::H2D, true),
        (PrinterModel::H2S, true),
        (PrinterModel::H2C, true),
        // Not gated upstream.
        (PrinterModel::X1E, true),
        (PrinterModel::H2DPro, true),
        (PrinterModel::A2L, true),
        (PrinterModel::X2D, true),
        (PrinterModel::Unknown, true),
    ];
    for (model, expected) in cases {
        assert_eq!(
            model.quirks().supports_ams_remote_drying(&ctx),
            expected,
            "{model:?} with nothing reported"
        );
    }
}

/// A version below each gated model's threshold is the only thing that turns it off.
#[test]
fn test_firmware_below_threshold_denies_only_gated_models() {
    let cases = [
        (PrinterModel::X1C, "01.08.99.99", false),
        (PrinterModel::P2S, "01.01.99.99", false),
        (PrinterModel::H2D, "01.02.29.99", false),
        (PrinterModel::H2S, "01.01.99.99", false),
        (PrinterModel::H2C, "01.01.99.99", false),
        // Ungated models ignore the version entirely.
        (PrinterModel::X1E, "00.00.00.01", true),
        (PrinterModel::H2DPro, "00.00.00.01", true),
        // Never-supported models stay false on any version.
        (PrinterModel::A1, "99.99.99.99", false),
        (PrinterModel::P1S, "99.99.99.99", false),
    ];
    for (model, firmware, expected) in cases {
        let ctx = QuirkContext::empty().with_firmware(Some(firmware));
        assert_eq!(
            model.quirks().supports_ams_remote_drying(&ctx),
            expected,
            "{model:?} at firmware {firmware}"
        );
    }
}

/// Each gated model allows exactly at its documented minimum.
#[test]
fn test_firmware_at_threshold_allows() {
    for (model, min) in [
        (PrinterModel::X1C, "01.09.00.00"),
        (PrinterModel::P2S, "01.02.00.00"),
        (PrinterModel::H2D, "01.02.30.00"),
        (PrinterModel::H2S, "01.02.00.00"),
        (PrinterModel::H2C, "01.02.00.00"),
    ] {
        let ctx = QuirkContext::empty().with_firmware(Some(min));
        assert!(
            model.quirks().supports_ams_remote_drying(&ctx),
            "{model:?} at its minimum {min}"
        );
    }
}

/// A reported `fun2` bit 5 outranks every model rule in both directions — including the
/// never-supported tier, which is the case most worth pinning since it is the one place a
/// reported bit could contradict a hardware fact.
#[test]
fn test_reported_bit_outranks_every_model_rule() {
    let set = QuirkContext::empty().with_fun2(Some("20"));
    let clear = QuirkContext::empty().with_fun2(Some("00"));

    for model in [
        PrinterModel::A1,
        PrinterModel::A1Mini,
        PrinterModel::P1P,
        PrinterModel::P1S,
        PrinterModel::X1C,
        PrinterModel::X1E,
        PrinterModel::H2D,
        PrinterModel::H2DPro,
        PrinterModel::H2S,
        PrinterModel::H2C,
        PrinterModel::P2S,
    ] {
        // A1/A1 Mini are the deliberate exception: their rule is a hardware fact about what
        // can be attached, not a firmware capability, so no reported bit overrides it.
        let expected_when_set = !matches!(model, PrinterModel::A1 | PrinterModel::A1Mini);
        assert_eq!(
            model.quirks().supports_ams_remote_drying(&set),
            expected_when_set,
            "{model:?} with fun2 bit 5 set"
        );
        assert!(
            !model.quirks().supports_ams_remote_drying(&clear),
            "{model:?} with fun2 bit 5 clear must refuse"
        );
    }
}

/// A reported bit beats firmware in both directions, so the two inputs cannot deadlock.
#[test]
fn test_reported_bit_beats_firmware() {
    let x1c = PrinterModel::X1C.quirks();

    // Bit set, firmware far too old: allowed.
    let old_but_reported = QuirkContext::empty()
        .with_fun2(Some("20"))
        .with_firmware(Some("01.00.00.00"));
    assert!(x1c.supports_ams_remote_drying(&old_but_reported));

    // Bit clear, firmware new enough: refused.
    let new_but_denied = QuirkContext::empty()
        .with_fun2(Some("00"))
        .with_firmware(Some("99.99.99.99"));
    assert!(!x1c.supports_ams_remote_drying(&new_but_denied));
}

/// A `fun2` string carrying no hex digits is "didn't say", not a reported zero, so the model
/// rules still decide. Distinguishing these two is what keeps an empty string from silently
/// disabling drying everywhere.
#[test]
fn test_empty_fun2_is_not_a_reported_zero() {
    let empty = QuirkContext::empty().with_fun2(Some(""));
    assert!(
        PrinterModel::X1C
            .quirks()
            .supports_ams_remote_drying(&empty)
    );
    assert!(
        !PrinterModel::P1S
            .quirks()
            .supports_ams_remote_drying(&empty)
    );

    let junk = QuirkContext::empty().with_fun2(Some("zz"));
    assert!(PrinterModel::X1C.quirks().supports_ams_remote_drying(&junk));
}
