# What counts as confirmation

**BambuStudio and bambuddy agreeing is confirmation — align with them.** Not "strong evidence pending hardware". State it plainly, drop the hedging, and change the code to match; there is nothing further to wait for. They are independent in the way that matters: BambuStudio is the vendor's own client, bambuddy an independent reverse-engineering of the same wire. A capture from our own printer corroborates and is worth citing, but is not required for a shape both already agree on.

**Those two specifically.** ha-bambulab is good supporting evidence and often the most readable account of a field, but it does not carry confirmation on its own or as the second source — a finding resting on ha-bambulab plus one other still needs BambuStudio and bambuddy checked. Cite it freely; don't count it.

- **One upstream is not two.** Where BambuStudio has an opinion, read it — checking only the more readable source looks thorough and isn't. **BambuStudio is authoritative when the two disagree**, and bambuddy marks its own guesses in its docstrings; take those at face value rather than inheriting them.
- **Read the whole call, not just the field in question.** Upstream frequently sends more than a finding describes, and matching the first source found reproduces the original defect one field over.
- **A parse site proves the field, not its unit — the unit may live at the call sites.** Grep the accessor as well as the assignment; upstream often stores a raw value and converts only where it renders it.
- **A capture proves presence, never absence.** A key missing from one model's payload says nothing about another model; that is what the upstreams are for. A key present in a capture is real regardless of what upstream does with it.
- **Don't soften a cleared claim to sound careful.** Hedging something already settled costs the next reader a full re-derivation.

`needs-verification` is for what this bar cannot close: physical behavior on a model nobody here has, or a wire shape no upstream implements. A *shape* confirmed by both upstreams is triageable even when the *harm* is unmeasured — that is a `P-low` footgun, not an open question.
