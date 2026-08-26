# Free-placement tile layout with a control palette

**Status:** approved design, pre-implementation
**Date:** 2026-08-26
**Supersedes:** the packer-driven grid (`order` + `shapes` in `[appearance]`)

## Problem

The grid today is *a set of controls, each with one shape, auto-packed*.
`[appearance] order` lists which tiles appear and in what sequence; `[appearance]
shapes` overrides a tile's shape; `tile_layout::pack` places them left-to-right
with no gaps. A control appears at most once.

The user wants:

1. **Duplicates** — the same control present several times, at different sizes
   (already partly true: a control and its size are a distinct thing).
2. **Free placement** — a tile sits exactly where it is dropped; **gaps are
   allowed and kept**. No auto-packing.
3. **A palette** in Settings: a horizontal, control-grouped, small→large picker
   to add tiles from.
4. **Direct manipulation** — the whole tile drags; remove via a red − on the
   tile (hover / right-click); the palette always offers a green + to add
   another.

This replaces the layout *model*, not just the Settings page.

## Decisions (from brainstorming)

- **Sizes per control:** what exists now, plus a **Wide** form for four square
  controls — Battery, DNS, Keep Awake, Keyboard Backlight. Sliders stay Wide;
  Connectivity stays Tall; square controls keep Half + Small and gain Wide.
- **Collisions on drop:** **refuse** — a drop lands only on fully-free cells,
  else the tile snaps back and the target flashes. Nothing the user did not
  drag ever moves.
- **Selection is derived:** a control's backend runs iff ≥1 instance of it is in
  the layout. The `[modules]` on/off switch list is **removed**. Non-tile
  toggles (Game Mode, charge limit, media row) keep a small switch list on the
  **Styling** tab.
  - *Connectivity subtlety:* the group's rows are Wi-Fi, Bluetooth and VPN, so
    the network/bluetooth/vpn backends run iff there is ≥1 instance of that
    control **or** ≥1 Connectivity instance. The group/standalone independence
    we built in 0.1.6 falls out of the instance model for free — they are just
    different controls you can place, together or apart — so the coupled flag
    logic (`show_connectivity`, the `|| connectivity` subscription guards)
    collapses into this one derived rule.
- **Gaps in the popup:** empty background, no ghost. The Settings grid still
  draws faint ghost slots so the drop targets are visible.
- **Add/remove semantics:** the **palette always adds** (green +, always, since
  you can always add another); you **remove from the grid** (red − on a tile).
  The palette is a source; the grid is what you edit.

## The model

### An instance, not a key

```rust
// tile_layout.rs
pub struct Instance {
    pub control: TileKey,     // reuses the existing enum
    pub shape: TileShape,     // Half | Small | Wide | Tall
    pub col: u16,             // 0..GRID_COLUMNS, top-left sub-column
    pub row: u16,             // 0.., top-left row
}
```

The layout is `Vec<Instance>`. Position is explicit; the packer is retired for
the popup and Settings render paths. `GRID_COLUMNS` stays 4 (a Small is 2
sub-columns), and `TileShape::columns/rows` are unchanged — the geometry a shape
occupies is the same; only *who chooses the position* changes.

### Config schema

```toml
[appearance]
style = "medium"

[[appearance.layout]]
control = "battery"
shape   = "small"
col     = 0
row     = 0

[[appearance.layout]]
control = "battery"
shape   = "wide"
col     = 0
row     = 1
```

`Appearance` loses `order` and `shapes`, gains `layout: Vec<Instance>`.

### Migration

On load, if `layout` is empty **and** (`order` or `shapes` is present, or this
is a fresh config), synthesise it:

1. Take the resolved order (the current `resolve_order` logic, kept for this
   one purpose).
2. For each key, its shape is `key.shape_with(&shapes)` — the existing rule.
3. Run the **existing packer** once to assign `(col, row)` to each, producing a
   gap-free starting layout identical to what 0.1.6 draws.
4. Write `layout` back; `order`/`shapes` are dropped on the next save.

So a 0.1.6 config opens looking exactly as it did, then becomes editable as
instances. `resolve_order` and `pack` survive as **migration-only** helpers, not
render-path code.

### Validation (where the bugs will live)

Free placement drops the packer's no-overlap / no-hole guarantee. Two guards
replace it, both in `tile_layout`:

- `fn validate(layout: &[Instance]) -> Vec<Instance>` — drops any instance that
  overlaps an earlier one or falls outside the 4-column grid. First-placed wins.
  Called on load (a hand-edited config can overlap) and after every edit before
  save. Returns the cleaned layout; logs what it dropped.
- `fn free_at(layout, shape, col, row) -> bool` — whether a shape's footprint is
  entirely unoccupied and in-bounds. This is what a drop consults.

Both are pure and table-tested; this is where the old packer's ~19 tests move
to, re-expressed as placement/overlap invariants.

## Rendering

### `tile_grid` becomes position-driven

Signature changes from `Vec<(Element, TileShape)>` (packed internally) to taking
placed instances. It maps `(col, row, shape)` straight onto the existing
band/strip cutter — the strip renderer already draws arbitrary placements; it
was only ever *fed* by the packer. So:

- `tile_grid(items: Vec<(Element, Instance)>, spacing) -> Element`
- Build a synthetic `Pack { tiles: placements-from-instances, ghosts, rows }`
  and reuse `bands()` unchanged. Ghosts = every cell no instance covers, one
  sub-column each (already how ghosts work).
- **Popup:** pass the ghosts as empty space (draw nothing).
- **Settings:** pass the ghosts as faint drop-target slots (the existing
  `ghost_tile`).

One flag on the render call decides ghost-visible vs ghost-empty. This keeps a
single layout engine for both surfaces — the property that has caught every
divergence bug this session.

### Instances address by index, not key

The popup builds `Vec<(Element, Instance)>` by walking `layout`. Two Batteries
are two elements. A tile's on-press / drill-down comes from its `control`;
nothing keys on uniqueness any more.

### Wide forms

Four new renderings, each a small design, in `ui/mod.rs` beside the existing
tile builders:

- **Battery Wide:** icon · `{percent}% · {profile} · {time}` on one line.
- **DNS Wide:** icon · provider name · servers in caption.
- **Keep Awake Wide:** icon · on/off or "Held by {who}".
- **Keyboard Wide:** icon · level name (Off/Low/Medium/High).

`app.rs` picks the builder by `(control, shape)`. A `Wide` override on a control
with no Wide form falls back to `Small` (mirrors how `Half` is ignored where it
makes no sense — one `shape_with`-style guard).

## Settings interaction

The `Tiles` tab is rebuilt around instances:

- **Grid**: draws `layout` with visible ghosts. Each tile:
  - drag from anywhere on it (`mouse_area` press + motion → a "held" instance
    following a target cell; drop calls `free_at`, commits or snaps back);
  - hover / right-click → red − overlay top-right → removes that instance.
- **Palette**: a horizontal `scrollable` of rows, one per control, each row its
  shapes small→large. Every item: a preview + green + → adds an instance at the
  first free cell (scan row-major for `free_at`), or is itself draggable onto a
  cell.
- No "Not shown" section — the palette replaces it. A control absent from the
  layout is simply available in the palette like every other.

### Messages (replacing the tap/drag set)

```
PickInstance(usize)         // start dragging the instance at this layout index
DragToCell(u16, u16)        // pointer over this cell while dragging
DropInstance                // commit if free_at, else snap back
RemoveInstance(usize)
AddFromPalette(TileKey, TileShape)
```

`ToggleModule` and the whole switch-list machinery are deleted. The Styling
tab grows a short switch list for the non-tile toggles.

## Files touched

- `src/tile_layout.rs` — `Instance`; `validate`, `free_at`, `first_free`;
  `pack`/`resolve_order` demoted to migration; band cutter unchanged; test
  suite re-centred on placement invariants.
- `src/config.rs` — `Appearance.layout`; migration from `order`/`shapes`;
  `deny_unknown_fields` still rejects typos; round-trip + migration tests.
- `src/ui/mod.rs` — `tile_grid` takes instances + a ghost-visible flag; four
  Wide builders; remove-overlay helper.
- `src/app.rs` — build `Vec<(Element, Instance)>` from `layout`; builder
  dispatch on `(control, shape)`; derived "is this control shown" replaces the
  `modules.*` reads in `subscription()`.
- `src/settings.rs` — palette + free-placement grid; new message set; Styling
  tab switch list for non-tile toggles.
- `data/config.example.toml`, `README.md`, `TESTING.md` — the new model.

## What is deliberately not done

- No per-control size beyond Half/Small/Wide (square) and the fixed slider /
  Connectivity shapes. No 2×2.
- No swap-on-collision, no push-on-collision. Refuse only.
- No drag between palette and grid *reordering the palette* — the palette order
  is fixed (control-grouped, small→large).
- Custom tiles keep their index-addressed model; they join the palette as one
  row.

## Risks

1. **Config migration correctness** — a 0.1.6 config must open identical. The
   migration reuses the real packer, so "identical" is by construction; a test
   loads a representative 0.1.6 config and asserts the resulting `layout` packs
   to the same cells.
2. **Hand-edited overlaps** — `validate` drops them first-wins and logs; the
   applet never renders overlapping tiles.
3. **Scope** — this is the largest single change to the applet. It lands as
   several commits (model+migration, render, Settings grid, palette, Wide
   forms), each built and verified before the next, not one big drop.
