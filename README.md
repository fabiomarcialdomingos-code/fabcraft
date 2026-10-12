# fabcraft

fabcraft automatically turns a drawn part into a fully dimensioned fabrication/shop drawing —
overall dimensions, a hole table, a material/thickness callout, and a sheet sized and scaled
for maximum legibility (not a fixed A4, not a fixed scale) — with one command. It targets the
real, everyday pain of CAD draftsmen producing manufacturing/production drawings: dimensioning
and view layout eat most of the clock on every part.

fabcraft is built **on top of [cadcraft](https://github.com/storytold/cadcraft)**, a clean-room,
open-source, pure-Rust CAD engine — fabcraft never forks or copies cadcraft's code. Its
`Cargo.toml` depends on cadcraft's crates as a pinned git dependency; bumping that pin is how
fabcraft pulls in cadcraft's future engine fixes and features for free. fabcraft adds only what's
new: the Part model and the automatic fabrication-drawing pipeline, in its own `fab-engine` crate.

fabcraft is conceptually inspired by the real commercial product **TecnoMETAL** (Steel & Graphics,
runs on BricsCAD) — but is a clean-room, independent implementation. No code, UI, icons, file
format or text from TecnoMETAL or any other commercial CAD is read, copied or reverse-engineered;
only the observable behaviour/concept is a reference point, the same relationship cadcraft already
holds with AutoCAD. Likewise, the structural-profile catalog and the bend-allowance (K-factor)
formula come from public engineering standards and published references, never from a commercial
tool's bundled data.

## What works today (v1 core, all tested)

- **The Part model** (`fab_engine::part`): a Part is a cadcraft block instance whose attributes
  carry its fabrication data (kind, thickness, material, catalog reference). The internal
  `PartData`/`PartKind` representation is decoupled from that storage mechanism on purpose — it
  can move to a different storage (e.g. XDATA) later without touching anything that reads it.
- **Smart drawing commands** (`plate`, `profile`, `tube`, `bent-profile`): draw the part's
  geometry and tag it as a Part in one atomic step. Freehand drawing with cadcraft's own
  `line`/`pline`/`circle` is completely untouched — nothing is imposed on the draftsman.
- **`fabdim` ("Cotar Desenho")** — the automatic dimensioning pipeline. **Fully implemented and
  tested for Plate parts**: overall width/height dimensions (real, associative cadcraft dimension
  entities — moving the geometry moves the dimension, for free, from cadcraft's own engine), a
  hole table, a thickness/material callout, and a sheet chosen by the layout algorithm below.
- **The sheet "hero sizing" algorithm** (`fab_engine::layout_fit`): picks the smallest standard
  sheet size and the clearest standard engineering scale (1:1, 1:2, 1:5, ... 1:200) that fits
  everything that needs to appear, instead of a fixed A4 or a fixed scale. Pure, exhaustively
  tested function, independent of any drawing side effect.
- **The flat-pattern (bend-allowance) math** for bent/sheet-metal parts (`fab_engine::commands::bent_profile`):
  the standard published formula `BA = angle × (radius + K·thickness)`, tested against hand-worked
  reference values. This was flagged in the design spec as the highest technical risk in v1 and
  got the most test coverage for exactly that reason.
- **`fabcraft-cli`**: a headless CLI (mirroring `cadcraft-cli`) that reaches every fabcraft command
  through the same JSON form the eventual UI/MCP layer will use — see "Try it" below.

Run `cargo test --workspace` — 34 tests, including the design spec's own worked example end to
end (a 100×100×5mm plate with 7 Ø10mm holes).

## What's explicitly not done yet (honest, not hidden)

- **Profile/Tube/BentProfile dimensioning.** Their smart drawing commands work, and (for
  BentProfile) the hard flat-pattern math is implemented and tested — but `fabdim` refuses them
  with a clear "not available yet" error rather than guessing at a template. Only Plate's
  dimensioning is real.
- **The native GUI.** fabcraft is CLI-first in this build. Reusing `cadcraft-ui-egui` for a real
  UI (toolbars for the smart tools, a BOM/hole-table panel, the single-question classification
  prompt for freehand geometry) is the next concrete step, not started.
- **Wiring the chosen sheet to an actual viewport view.** `fabdim` creates the destination layout
  and reports the chosen sheet size and scale in its JSON result — the sheet-choice *decision*
  (the actual "hero sizing" logic) is complete and tested. Panning/zooming the layout's viewport
  to physically match that choice (cadcraft's `view.set`/`mview` paper-space machinery) is not
  wired up yet; verifying that integration correctly was out of this session's budget, and
  shipping unverified integration code would be worse than flagging the gap.
- **The classification question for freehand geometry.** Smart tools always tag a Part
  automatically. Plain `line`/`pline`/`circle` geometry does not — a one-question flow that fills
  the same fields `fabdim` could then use is designed (see the spec) but not implemented.
- **A fab command's undo is not atomic.** `plate`/`profile`/`tube`/`bent-profile`/`fabdim` each
  compose several native cadcraft commands (`pline`, `attdef`, `block`, `dimlinear`, `table`...).
  Each of those gets cadcraft's own real undo step and panic guard — but cadcraft's internal
  undo/assoc bookkeeping that would merge them into one step is private to the cadcraft-engine
  crate and out of reach from here. Undoing a `plate` call takes a few Ctrl+Z, not one.
- **Scale/rotation edge case.** `fabdim`'s world-coordinate reconstruction assumes a Part was
  only ever translated (moved) after creation, not scaled or rotated by a general edit — correct
  for the normal "draw, then dimension" flow, a known gap otherwise.
- The profile catalog (`fab_engine::catalog`) has a handful of illustrative sizes, not a complete
  standard-shapes table.

## Try it

```
cargo test --workspace

cargo run -p fabcraft-cli -- run --sample \
  --cmd 'plate {"width":100,"height":100,"thickness":5,"material":"A36","holes":[{"center":[20,20],"diameter":10},{"center":[80,80],"diameter":10}]}' \
  --cmd 'fabdim {"handle":"<the handle plate just printed>"}' \
  --save out.dxf

cargo run -p fabcraft-cli -- commands        # every cadcraft command, plus fabcraft's own (marked "(fab)")
```

## Repository layout

```
Cargo.toml                 workspace; pins cadcraft's crates as a git dependency
crates/fab-engine/          the Part model, smart commands, fabdim, layout_fit, catalog
apps/fabcraft-cli/          headless CLI (mirrors cadcraft-cli)
```

To pull in a newer cadcraft, bump the `rev` on every `cadcraft-*` entry in the root `Cargo.toml`
to the commit you want, then `cargo check --workspace`.

## Non-negotiables (carried over from cadcraft, unchanged)

- **Never crash.** No `unwrap()`/`expect()`/`panic!()`/`unsafe` in production code
  (`#![forbid(unsafe_code)]`, `#![deny(clippy::unwrap_used, clippy::expect_used, ...)]` on every
  crate root — relaxed only inside `#[cfg(test)]`, where a panicking assertion is just a normal
  test failure, not a production crash). Hostile/missing input is always a clear `Result` error.
- **Clean-room.** Never read, copy or reverse-engineer TecnoMETAL, any other commercial CAD, or
  any GPL/LGPL/AGPL project. Catalog data and formulas come from public engineering references.
- **Everything is a command**, following cadcraft's own convention: a `CommandSpec` with a JSON
  form and an id that's never an AutoCAD command name (so fabcraft's own registry never collides
  with cadcraft's).
