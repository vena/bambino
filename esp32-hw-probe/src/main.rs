//! Current investigation: GitHub issue #65's `EspIdfTimer` fix.
//!
//! `TimerProvider::sleep` used to hold a `RefCell` borrow of the shared
//! `EspAsyncTimer` across its await point. Because `sleep` takes `&self` and one
//! timer is shared by a whole client, two sleeps live on the same executor abort
//! the task with `BorrowMutError`. The fix moves the timer out of the cell before
//! the await and restores it after, so a concurrent caller sees `None` and
//! allocates its own timer instead of panicking.
//!
//! Clippy and `cargo check` can't observe any of that. These four checks can:
//!
//! 1. two sleeps racing on one shared timer both complete (the defect itself);
//! 2. sustained racing doesn't leak `esp_timer` slots (the restore-if-empty path);
//! 3. a sleep future dropped mid-await leaves the timer usable (cancellation);
//! 4. sleeps still take roughly as long as asked (the fix didn't break timing).
//!
//! Prior investigations (e.g. BUG-051's timer-exhaustion stress test) are
//! recoverable via `git log -- esp32-hw-probe/src/main.rs`, not kept live here.

use bambino::io::esp_idf::EspIdfTimer;
use bambino::io::TimerProvider;
use core::time::Duration;

/// Iterations for the slot-leak check. BUG-051 established that 10,000
/// allocate/drop cycles are safe, so anything failing here is the restore path
/// leaking, not the `esp_timer` slot cap being tight.
const CONTENTION_ITERATIONS: u32 = 500;
const CONTENTION_SLEEP: Duration = Duration::from_millis(20);

fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("esp32-hw-probe: issue #65 EspIdfTimer concurrent-sleep probe");

    let timer = match EspIdfTimer::new() {
        Ok(timer) => timer,
        Err(e) => {
            log::error!("FAIL setup: EspIdfTimer::new() failed: {e:?}");
            park();
        }
    };

    let mut failures = 0u32;

    // 1. The defect: two sleeps racing on one shared timer. Pre-fix this aborts
    //    the task with BorrowMutError rather than returning at all, so reaching
    //    the log line below is itself most of the result.
    let start = timer.now_millis();
    let (a, b) = esp_idf_svc::hal::task::block_on(embassy_futures::join::join(
        timer.sleep(Duration::from_millis(300)),
        timer.sleep(Duration::from_millis(300)),
    ));
    let elapsed = timer.now_millis().saturating_sub(start);
    if a.is_err() || b.is_err() {
        log::error!("FAIL concurrent: sleeps returned {a:?} / {b:?}");
        failures += 1;
    } else if elapsed >= 550 {
        // Both should run concurrently (~300ms). Serialization (~600ms) would mean
        // the second caller is somehow waiting on the first's timer.
        log::error!("FAIL concurrent: {elapsed}ms elapsed, expected ~300ms (serialized?)");
        failures += 1;
    } else {
        log::info!("PASS concurrent: both sleeps completed, {elapsed}ms elapsed");
    }

    // 2. Sustained contention. Each iteration forces one fallback allocation; if
    //    the restore-if-empty path is wrong, slots accumulate until allocation
    //    starts failing partway through.
    let mut contention_failures = 0u32;
    for i in 0..CONTENTION_ITERATIONS {
        let (a, b) = esp_idf_svc::hal::task::block_on(embassy_futures::join::join(
            timer.sleep(CONTENTION_SLEEP),
            timer.sleep(CONTENTION_SLEEP),
        ));
        if a.is_err() || b.is_err() {
            log::error!("FAIL contention: iteration {i} returned {a:?} / {b:?}");
            contention_failures += 1;
            break;
        }
    }
    if contention_failures == 0 {
        log::info!("PASS contention: {CONTENTION_ITERATIONS} racing sleep pairs, no failures");
    } else {
        failures += contention_failures;
    }

    // 3. Cancellation: `select` drops the slower future mid-await, which drops the
    //    timer it took out of the cell. The next sleep must reallocate and work.
    esp_idf_svc::hal::task::block_on(async {
        let _ = embassy_futures::select::select(
            timer.sleep(Duration::from_millis(1000)),
            timer.sleep(Duration::from_millis(50)),
        )
        .await;
    });
    match esp_idf_svc::hal::task::block_on(timer.sleep(Duration::from_millis(50))) {
        Ok(()) => log::info!("PASS cancellation: timer still usable after a dropped sleep"),
        Err(e) => {
            log::error!("FAIL cancellation: sleep after a dropped sleep returned {e:?}");
            failures += 1;
        }
    }

    // 4. Timing sanity — a fix that returned instantly would pass everything above.
    let start = timer.now_millis();
    let result = esp_idf_svc::hal::task::block_on(timer.sleep(Duration::from_millis(500)));
    let elapsed = timer.now_millis().saturating_sub(start);
    if result.is_ok() && (450..=650).contains(&elapsed) {
        log::info!("PASS timing: 500ms sleep took {elapsed}ms");
    } else {
        log::error!("FAIL timing: 500ms sleep returned {result:?} after {elapsed}ms");
        failures += 1;
    }

    if failures == 0 {
        log::info!("SUMMARY: all checks passed");
    } else {
        log::error!("SUMMARY: {failures} check(s) failed");
    }

    park();
}

/// ESP-IDF's `main` isn't meant to return — spin so the monitor keeps the log
/// visible until Ctrl-C.
fn park() -> ! {
    loop {
        esp_idf_svc::hal::delay::FreeRtos::delay_ms(1000);
    }
}
