// Placeholder — see README.md for the reuse convention. Prior investigations
// (e.g. BUG-051's timer-exhaustion stress test) are recoverable via
// `git log -- esp32-hw-probe/src/main.rs`, not kept live here.

fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("esp32-hw-probe: no active investigation");

    loop {
        esp_idf_svc::hal::delay::FreeRtos::delay_ms(1000);
    }
}
