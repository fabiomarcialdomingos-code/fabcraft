# fabcraft

fabcraft automatically turns a drawn part into a fully dimensioned fabrication/shop drawing —
overall dimensions, a hole table, a material/thickness callout, and a sheet sized and scaled
for maximum legibility (not a fixed A4, not a fixed scale) — with one command. It targets the
real, everyday pain of CAD draftsmen producing manufacturing/production drawings: dimensioning
and view layout eat most of the clock on every part.

fabcraft is built **on top of [cadcraft](https://github.com/storytold/cadcraft)**, a clean-room,
open-source, pure-Rust CAD engine — fabcraft never forks or copies cadcraft's code. Its
`Cargo.toml` depends on cadcraft's crates (including the full `cadcraft-ui-egui` interface) as a
pinned git dependency; bumping that pin is how fabcraft pulls in cadcraft's future engine fixes
and features for free. fabcraft adds only what's new: the Part model and the automatic
fabrication-drawing pipeline, in its own `fab-engine` crate, plus a thin app layer.

fabcraft is conceptually inspired by the real commercial product **TecnoMETAL** (Steel & Graphics,
runs on BricsCAD) — but is a clean-room, independent implementation. No code, UI, icons, file
format or text from TecnoMETAL or any other commercial CAD is read, copied or reverse-engineered;
only the observable behaviour/concept is a reference point, the same relationship cadcraft already
holds with AutoCAD. Likewise, the structural-profile catalog and the bend-allowance (K-factor)
formula come from public engineering standards and published references, never from a commercial
tool's bundled data.

## What works today (v1 core, all tested)

- **The Part model** (`fab_engine::part`): a Part is a cadcraft block instance whose attributes
  carry its fabrication data (kind, thickness, material, catalog reference, and — for bent
  profiles — bend radius/K-factor). The internal `PartData`/`PartKind` representation is decoupled
  from that storage mechanism on purpose — it can move to a different storage (e.g. XDATA) later
  without touching anything that reads it.
- **Smart drawing commands** (`plate`, `profile`, `tube`, `bent-profile`): draw the part's
  geometry and tag it as a Part in one atomic step. Freehand drawing with cadcraft's own
  `line`/`pline`/`circle` is completely untouched — nothing is imposed on the draftsman.
- **`fabclassify`** — the single-question path for geometry a smart tool never touched: turns
  already-drawn freehand entities into a real Part by filling the same fields a smart tool fills
  in automatically. Proven to make `fabdim` accept the result exactly like a smart-tool part.
- **`fabdim` ("Cotar Desenho")** — the automatic dimensioning pipeline, **implemented and tested
  for all four PartKinds**:
  - *Plate*: overall width/height, a hole table, a thickness/material callout.
  - *Profile/Tube*: overall length, a hole table, and — when the part references a catalog
    section — a cross-section mini-view drawn from the catalog's own dimensions (never fabricated
    for a free/uncatalogued section).
  - *BentProfile*: per-flange dimensions and bend-angle callouts on the folded view, plus the
    flat/unfolded view at the recomputed flat-pattern length. Flange lengths and bend angles are
    read back from the folded polyline's own geometry (no separate storage needed); only bend
    radius and K-factor are persisted on the Part. A test proves `fabdim` recovers the *exact* flat
    length the drawing command itself computed at creation time, from geometry alone.

  Every dimension fabdim creates is a real, associative cadcraft dimension entity — moving the
  geometry moves the dimension, for free, from cadcraft's own engine (`DIMASSOC`).
- **The sheet "hero sizing" algorithm** (`fab_engine::layout_fit`), **wired to a real viewport**:
  picks the smallest standard sheet size and the clearest standard engineering scale (1:1, 1:2,
  1:5, ... 1:200) that fits everything that needs to appear, then creates the layout and points its
  viewport at the part using cadcraft's own `viewport.set` (`scale: "1:N"`, `center`) — not just a
  reported decision, an actually-generated sheet.
- **The flat-pattern (bend-allowance) math** for bent/sheet-metal parts (`fab_engine::commands::bent_profile`):
  the standard published formula `BA = angle × (radius + K·thickness)`, tested against hand-worked
  reference values. This was flagged in the design spec as the highest technical risk in v1 and
  got the most test coverage for exactly that reason.
- **A desktop GUI** (`apps/fabcraft`): the full `cadcraft-ui-egui` interface — drawing, dimensions,
  blocks, hatch, layouts, plotting, all of it — reused wholesale and unmodified, plus a side panel
  with real input forms for fabcraft's own commands (`apps/fabcraft/src/forms.rs`): editable
  width/height/thickness/material and an add/remove hole list for Plate, a catalog picker
  (`ComboBox`, populated from `fab_engine::catalog::names()`) or a free-height field for
  Profile/Tube, flange/bend-angle/thickness/K-factor fields for BentProfile, and a "classify the
  current selection" form that reads `CadApp.session.selection()` live. See "Why a side panel, not
  new menu entries" below for why it's a side panel. Verified with real screenshots, not just "it
  compiles": launches and closes cleanly, and every field renders fully visible beside cadcraft's
  own Layers/Properties docks (the first cut's window size overflowed this machine's display and
  clipped the panel — fixed after a screenshot caught it).
- **Atomic undo.** A fab command composes several native cadcraft calls internally (`pline`,
  `attdef`, `block`, ...) — `fab_engine::dispatch::execute` snapshots the document first, lets
  those sub-calls push their own steps as usual, then on success collapses them into the single
  undo step a fab command should be (`DocState::undo`/`push_undo` turned out to be fully public,
  not sealed as a first pass assumed). Proven by a test: drawing a plate pushes exactly one undo
  entry, and undoing it once fully restores the pre-plate drawing.
- **Correct dimensioning after a general edit.** `fabdim` reconstructs a Part's world-space
  geometry with the real cadcraft INSERT transform (translation, independent X/Y scale, rotation —
  verified against `EntityKind::transform`'s own semantics by reading it, not guessing), not a
  translation-only shortcut. Proven end to end: a plate scaled 2x and rotated 90° by cadcraft's
  real `scale`/`rotate` commands still dimensions correctly, checked by reading the created
  dimension and hole-table entities back and comparing to hand-worked expected values.
- **A GUI command line for fab commands.** The side panel has its own `id {json}` box (the same
  form `fabcraft-cli` takes), going through the same `dispatch::execute` — so it reaches fab
  commands *and* falls through to any native cadcraft command, from one box.
- **`fabcraft-cli`**: a headless CLI (mirroring `cadcraft-cli`) that reaches every fabcraft command
  through the same JSON form the GUI and any future MCP layer use.

Run `cargo test --workspace` — 46 tests, including the design spec's own worked example end to
end (a 100×100×5mm plate with 7 Ø10mm holes), the bent-profile flat-length round trip, the
single-undo-step proof, and the scale+rotate dimensioning proof.

### Why a side panel, not new entries in cadcraft's own menus/command line

cadcraft-engine's command registry (`find_command`) is a private static list sealed inside that
crate, and cadcraft-ui-egui's own command dispatch (`CadApp::run`/`cmdline`) calls into it from
roughly 18 different files spread across that crate. There is no single point to make either one
aware of fabcraft's commands without forking cadcraft-ui-egui — which the whole point of a pinned
git dependency (zero fork, zero copy) rules out. `CadApp.session` is a public field, though, so
fabcraft's side panel calls `fab_engine::dispatch::execute` on it directly, bypassing `CadApp::run`
entirely — a clean extension point that needed no change to cadcraft's code at all. The GUI panel's
own command-line box (above) is how fab commands get a typed entry point despite this, without
needing one in cadcraft's own command line.

## What's explicitly not done yet (honest, not hidden)

- The profile catalog (`fab_engine::catalog`) has 24 sizes across four families (U, W, L, tube) —
  substantially more than the first cut's 6, but still not, and never really able to be, a
  "complete" table of every structural shape that exists; the free/uncatalogued path (`catalogRef`
  simply omitted) is how anything not in it gets drawn and dimensioned anyway.
- No parametric structural-element library, weld lists/connection tables, sheet nesting, or
  CAM/NC output — all explicitly phase 2+ per the design spec, not v1 scope at all.

## Try it

```
cargo test --workspace

cargo run -p fabcraft-cli -- run --sample \
  --cmd 'plate {"width":100,"height":100,"thickness":5,"material":"A36","holes":[{"center":[20,20],"diameter":10},{"center":[80,80],"diameter":10}]}' \
  --cmd 'fabdim {"handle":"<the handle plate just printed>"}' \
  --save out.dxf

cargo run -p fabcraft-cli -- commands        # every cadcraft command, plus fabcraft's own (marked "(fab)")
cargo run -p fabcraft                        # the desktop GUI
```

### Windows build setup

Needs a Rust toolchain and, on Windows, a linker for the crates that bind to native Windows APIs
(`windows-sys` and friends, pulled in by `eframe`/`wgpu` for the GUI). Either works:

- **GNU (lighter, what this repo was developed with):** `rustup toolchain install stable-x86_64-pc-windows-gnu && rustup default stable-x86_64-pc-windows-gnu`,
  plus a MinGW-w64 distribution on `PATH` for `dlltool.exe` (e.g. `winget install BrechtSanders.WinLibs.POSIX.UCRT`).
  Without it, `cargo build -p fabcraft` fails at `dlltool.exe: program not found` — `fab-engine`
  and `fabcraft-cli` alone (no GUI) don't need this.
- **MSVC (what cadcraft's own official Windows binaries use):** install the "Desktop development
  with C++" workload via Visual Studio Build Tools; no separate MinGW install needed.

## Repository layout

```
Cargo.toml                 workspace; pins cadcraft's crates as a git dependency
crates/fab-engine/          the Part model, smart commands, fabdim, layout_fit, catalog, dispatch
apps/fabcraft-cli/          headless CLI (mirrors cadcraft-cli)
apps/fabcraft/              desktop GUI (cadcraft-ui-egui, reused, plus the fabcraft side panel)
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
