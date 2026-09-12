---
paths:
  - "src/quirks/**"
  - "src/client/capabilities.rs"
---

Firmware-version thresholds in `src/quirks/` must be sourced from Bambu Lab's own documentation, not ported from one upstream table. Three drying constants were wrong at once (#272, #273, #274) for exactly that reason. The resolved values and the rejected ones are tabulated in `reference/05_materials_ams.md` §5.4 ("Remote-drying firmware thresholds").

**Source order, strongest first** (extends the backlog skill's two-upstream bar for version numbers only):

1. The model's firmware release history — `https://wiki.bambulab.com/en/<model>/manual/<model>-firmware-release-history`. Exceptions: `h2d-pro/manual/firmware-release-history`, `x1/manual/X1-X1C-firmware-release-history`, `x1/manual/X1E-firmware-release-history`.
2. A Bambu feature wiki page with a minimum-firmware table (e.g. *Filament drying guide for AMS 2 Pro and AMS HT*). The only source that states non-support outright; can lag the per-model notes.
3. BambuStudio release notes — corroborating; they name builds absent from the public history.
4. BambuStudio and bambuddy source agreeing.
5. A single upstream table — only when nothing above speaks, and the doc comment must say so.

**Traps that actually happened:**

- A real vendor number filed against the wrong feature: `01.02.30.00` is Studio 2.5.0's dry-*while-printing* minimum, not idle drying, and isn't in the H2D history.
- "Added support for AMS 2 Pro/HT" misread as the capability arriving: X1 `01.09.00.00` and P1 `01.08.00.00` both say drying starts "from the printer's screen".
- A beta build: ha-bambulab's X1 `01.08.50.18`. A `.50.` segment is the tell.
- An unfalsifiable citation: bambuddy's drying tables cite "Bambu wiki release notes" with no page, and two such entries fail against the pages they must mean.

**Every firmware-version constant's doc comment names its page or release** — a URL or exact page title plus version and date. Never "the wiki" or "upstream". `reference/firmware-histories/` and `reference/wiki-pages/` are untracked local copies; cite the vendor page, not those paths.
