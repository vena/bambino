# esp32-hw-probe

Flashable ESP-IDF harness for hardware questions about `src/io/esp_idf.rs`.
`README.md` covers why it exists, how to flash and run it, and chip
retargeting; this file holds what an agent changing it needs.

**To reuse for a new investigation:** replace `src/main.rs`'s body and keep the
`esp_idf_svc::sys::link_patches()` / logger-init boilerplate at the top. The
file holds only the *current* investigation; `git log --
esp32-hw-probe/src/main.rs` is the record of earlier ones. Drive `bambino`'s
shipped types through the path dependency rather than copying them: a copy can
pass while the real code still fails.

**Concurrency means futures, not threads.** `&self`-taking types holding a
`RefCell` (e.g. `EspIdfTimer`) aren't `Sync`, so "two callers at once" has to be
two futures on one executor; `embassy-futures`' `join`/`select` are already
dependencies for that.

**Credentials:** copy `.env.example` to `.env` (gitignored). `PROBE_ACCESS_CODE`
and `PROBE_RESET_LISTENER` are optional and must be read with `option_env!`,
not `env!` (see `build.rs`). Scrub the serial and access code from any log
before it enters the repo.

**Don't self-verify.** The rule in
`.claude/rules/wire-framing-hardware-verification.md` applies to everything this
probe measures: hand the transcript to the user.
