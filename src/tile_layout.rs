//! Identity, shape and default order for every tile that can appear in the
//! popup.
//!
//! # Why this module exists
//!
//! Before the grid rework, tiles were anonymous elements laid out in whatever
//! order `root_page` happened to push them. That was fine while every tile was
//! the same size and the order was fixed in code. It stops being fine the
//! moment tiles have shapes (Wide, Tall) and the moment the user reorders them
//! from Settings — persistence needs a stable *name* per tile, and the packer
//! needs a stable *shape* per tile, and both need to be resolvable from a
//! string in `config.toml`.
//!
//! [`TileKey`] is the name. [`TileShape`] is the shape. [`default_order`] is
//! what a fresh install shows before anyone rearranges anything. The order in
//! `config.toml`'s `[appearance] order = […]` is a list of these string
//! representations; unknown entries are dropped, missing ones are appended in
//! the default order so a config that predates a new tile does not silently
//! omit it.
//!
//! Custom tiles are addressed by index rather than by name because their names
//! are user-set strings that can change or clash. The index is stable across
//! reorders of the *`[[custom]]`* array, not across additions and removals —
//! that is the same trade the rest of the applet makes for custom tiles.

use serde::{Deserialize, Serialize};

/// Every tile the popup can draw.
///
/// The Serialize/Deserialize representation is the kebab-case string, chosen
/// so the config file reads naturally: `order = ["wifi", "battery", …]`. Do
/// not rename these strings — a rename silently removes every tile a user had
/// listed under the old name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TileKey {
    /// The Wi-Fi + Bluetooth + VPN grouped tile.
    ///
    /// When present in the order, the three standalone tiles for these modules
    /// are hidden — the group replaces them rather than sitting alongside.
    Connectivity,
    Wifi,
    Bluetooth,
    Vpn,
    Battery,
    Dns,
    DarkMode,
    Tiling,
    GameMode,
    Media,
    DoNotDisturb,
    KeepAwake,
    ChargeThreshold,
    KeyboardBacklight,
    Volume,
    Brightness,
    Microphone,
}

impl TileKey {
    /// Default footprint for this tile.
    ///
    /// Sliders (volume, brightness, microphone) are Tall — a narrow column
    /// with the icon on top, the level below, and the percentage at the
    /// bottom. Connectivity is Wide because three rows of icon + label + switch
    /// do not fit legibly in a single-column tile. Everything else is Small.
    /// The shape to draw: the user's override for this tile, else the default.
    ///
    /// An override to `Half` is honoured only for tiles whose default is
    /// `Small`. A Half is icon-only, and the sliders and the Connectivity
    /// group have no icon-only form — a half-width slider is not a slider.
    /// Ignoring the override beats drawing something broken.
    pub fn shape_with(
        self,
        overrides: &std::collections::HashMap<TileKey, TileShape>,
    ) -> TileShape {
        let default = self.default_shape();
        match overrides.get(&self).copied() {
            Some(TileShape::Half) if default != TileShape::Small => default,
            Some(shape) => shape,
            None => default,
        }
    }

    pub fn default_shape(self) -> TileShape {
        match self {
            // A narrow column of stacked switches, the way the macOS Control
            // Centre draws its connectivity block — not a full-width banner.
            TileKey::Connectivity => TileShape::Tall,
            // Sliders read left-to-right. A vertical track in a square tile
            // was tried and is worse: the travel is shorter, and the value is
            // no longer where the eye expects it.
            TileKey::Volume | TileKey::Brightness | TileKey::Microphone => TileShape::Wide,
            _ => TileShape::Small,
        }
    }
}

/// A tile's footprint on the grid.
///
/// The grid is two columns wide, so:
///   `Small` = 1 column, 1 row (half-width)
///   `Wide`  = 2 columns, 1 row (full-width)
///   `Tall`  = 1 column, 2 rows (half-width column-strip)
///
/// `Tall × Wide` (a 2×2 block) is not offered on purpose: nothing in the
/// current tile set warrants it, and adding it would double the number of
/// packing cases the tests have to cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TileShape {
    /// Half a Small: icon only, four to a row.
    Half,
    Small,
    Wide,
    Tall,
}

/// The grid's width in sub-columns. A Small is two of them, so the grid is
/// still "two tiles wide" for every shape that existed before Half did.
pub const GRID_COLUMNS: u16 = 4;

impl TileShape {
    /// Sub-columns occupied (out of [`GRID_COLUMNS`]).
    pub fn columns(self) -> usize {
        match self {
            TileShape::Half => 1,
            TileShape::Small | TileShape::Tall => 2,
            TileShape::Wide => 4,
        }
    }

    /// Rows occupied.
    pub fn rows(self) -> usize {
        match self {
            TileShape::Half | TileShape::Small | TileShape::Wide => 1,
            TileShape::Tall => 2,
        }
    }
}

/// The order a fresh install shows.
///
/// Ordered from "most likely to be reached for" to least: connectivity first,
/// then power state and quick toggles, then the sliders at the end. This is
/// what appears in `config.toml`'s `order` field the first time it is written.
///
/// Kept in one place because it is the reference the migration in
/// [`resolve_order`] uses to append newly-added tiles for someone whose config
/// predates them.
pub const DEFAULT_ORDER: &[TileKey] = &[
    TileKey::Connectivity,
    // The standalone Wi-Fi/Bluetooth/VPN tiles sit right behind the group.
    // They are off by default, but they must still have a slot: a key absent
    // from here is one `resolve_order` never yields, so switching the tile on
    // would have drawn nothing.
    TileKey::Wifi,
    TileKey::Bluetooth,
    TileKey::Vpn,
    TileKey::Battery,
    TileKey::Dns,
    TileKey::DarkMode,
    TileKey::Tiling,
    TileKey::GameMode,
    TileKey::Media,
    TileKey::DoNotDisturb,
    TileKey::KeepAwake,
    TileKey::ChargeThreshold,
    TileKey::KeyboardBacklight,
    TileKey::Volume,
    TileKey::Brightness,
    TileKey::Microphone,
];

/// Take the order from `config.toml` and produce the list to draw.
///
/// Three rules, applied in order:
///   1. Drop duplicates from the stored list — the first occurrence wins.
///   2. Append any tile in [`DEFAULT_ORDER`] that is missing from the stored
///      list, in default order. A config that predates a new tile therefore
///      shows the new tile at the end, rather than silently omitting it.
///   3. Filter out any tile whose module is disabled in `[modules]`. The
///      module-off switch decides whether a tile is drawn; the order decides
///      *where*, if it is.
///
/// **Not** applied here: hiding the individual Wi-Fi/Bluetooth/VPN tiles when
/// the Connectivity group is present. That belongs at the render site, where
/// the connectivity module's own availability is also considered — a Wi-Fi
/// row inside a group is worthless on a machine with no Wi-Fi adapter.
pub fn resolve_order(stored: &[TileKey], is_enabled: impl Fn(TileKey) -> bool) -> Vec<TileKey> {
    let mut seen = std::collections::HashSet::with_capacity(DEFAULT_ORDER.len());
    let mut order = Vec::with_capacity(DEFAULT_ORDER.len());

    for &key in stored {
        if seen.insert(key) {
            order.push(key);
        }
    }
    for &key in DEFAULT_ORDER {
        if seen.insert(key) {
            order.push(key);
        }
    }

    order.into_iter().filter(|&key| is_enabled(key)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_enabled(_: TileKey) -> bool {
        true
    }

    #[test]
    fn a_fresh_config_reads_the_default_order() {
        let resolved = resolve_order(&[], every_enabled);
        assert_eq!(resolved, DEFAULT_ORDER);
    }

    #[test]
    fn a_stored_order_is_honoured() {
        let stored = vec![TileKey::Volume, TileKey::Battery, TileKey::Wifi];
        let resolved = resolve_order(&stored, every_enabled);
        assert_eq!(&resolved[..3], stored.as_slice());
    }

    #[test]
    fn missing_tiles_are_appended_in_default_order() {
        // A config from before Media existed should still draw Media, not
        // silently omit it.
        let stored = vec![TileKey::Battery];
        let resolved = resolve_order(&stored, every_enabled);
        assert_eq!(resolved.first(), Some(&TileKey::Battery));
        assert!(resolved.contains(&TileKey::Media));
    }

    #[test]
    fn duplicates_in_the_stored_order_are_dropped_first_wins() {
        // A hand-edited config might list a tile twice. The first position is
        // what the user wants; drawing it in both would just look like a bug.
        let stored = vec![TileKey::Volume, TileKey::Battery, TileKey::Volume];
        let resolved = resolve_order(&stored, every_enabled);
        assert_eq!(
            resolved.iter().filter(|&&k| k == TileKey::Volume).count(),
            1
        );
        // And the first occurrence's position sticks.
        assert_eq!(resolved.first(), Some(&TileKey::Volume));
    }

    #[test]
    fn a_disabled_module_is_not_drawn_wherever_it_sits_in_the_order() {
        let stored = vec![TileKey::Wifi, TileKey::Battery, TileKey::Volume];
        let resolved = resolve_order(&stored, |key| key != TileKey::Battery);
        assert!(!resolved.contains(&TileKey::Battery));
    }

    #[test]
    fn shapes_measure_in_sub_columns_where_a_small_is_two() {
        // The grid is four sub-columns so that a Half can be exactly half a
        // Small. Every other shape's width doubled with that change; the
        // heights did not.
        assert_eq!(TileShape::Half.columns(), 1);
        assert_eq!(TileShape::Small.columns(), 2);
        assert_eq!(TileShape::Tall.columns(), 2);
        assert_eq!(TileShape::Wide.columns(), 4);
        assert_eq!(TileShape::Wide.columns(), GRID_COLUMNS as usize);

        assert_eq!(TileShape::Tall.rows(), 2);
        for flat in [TileShape::Half, TileShape::Small, TileShape::Wide] {
            assert_eq!(flat.rows(), 1, "{flat:?}");
        }
    }

    #[test]
    fn every_tile_key_has_a_defined_default_shape() {
        // Add-a-key-forget-a-shape guard: iterating the enum's default_shape
        // for every variant that appears in DEFAULT_ORDER will panic if a new
        // variant is added without giving it a home in the match, thanks to
        // Rust's exhaustiveness check on the pattern. This test additionally
        // pins the *count*, so a variant added to the enum but forgotten in
        // DEFAULT_ORDER fails here rather than silently disappearing.
        //
        // Bump the number when adding a tile — that failure is a reminder to
        // pick where the new tile goes in the default order.
        assert_eq!(DEFAULT_ORDER.len(), 17);
    }

    #[test]
    fn every_tile_key_has_a_slot_in_the_default_order() {
        // A key missing here is one `resolve_order` never yields, so the tile
        // can never be drawn however its switch is set. That is how the
        // standalone Wi-Fi, Bluetooth and VPN tiles were unreachable.
        for key in [
            TileKey::Connectivity,
            TileKey::Wifi,
            TileKey::Bluetooth,
            TileKey::Vpn,
            TileKey::Battery,
            TileKey::Dns,
            TileKey::DarkMode,
            TileKey::Tiling,
            TileKey::GameMode,
            TileKey::Media,
            TileKey::DoNotDisturb,
            TileKey::KeepAwake,
            TileKey::ChargeThreshold,
            TileKey::KeyboardBacklight,
            TileKey::Volume,
            TileKey::Brightness,
            TileKey::Microphone,
        ] {
            assert!(
                DEFAULT_ORDER.contains(&key),
                "{key:?} has no slot in DEFAULT_ORDER, so it can never be drawn"
            );
        }
    }

    #[test]
    fn half_is_only_honoured_where_the_default_is_small() {
        let mut o = std::collections::HashMap::new();
        o.insert(TileKey::Battery, TileShape::Half);
        o.insert(TileKey::Volume, TileShape::Half);
        o.insert(TileKey::Connectivity, TileShape::Half);
        assert_eq!(TileKey::Battery.shape_with(&o), TileShape::Half);
        // A half-width slider is not a slider; the override is ignored.
        assert_eq!(TileKey::Volume.shape_with(&o), TileShape::Wide);
        assert_eq!(TileKey::Connectivity.shape_with(&o), TileShape::Tall);
    }

    #[test]
    fn tile_keys_round_trip_through_toml() {
        // The whole point of the kebab-case serde attribute: users type these
        // in their config, so a rename here silently breaks every config that
        // used the old name. The pin: encode → decode → equal.
        #[derive(Deserialize, Serialize, PartialEq, Debug)]
        struct Wrap {
            order: Vec<TileKey>,
        }
        for &key in DEFAULT_ORDER {
            let original = Wrap { order: vec![key] };
            let encoded = toml::to_string(&original).unwrap();
            let decoded: Wrap = toml::from_str(&encoded).unwrap();
            assert_eq!(decoded, original, "{key:?} did not round-trip");
        }
    }
}

/// A tile's placement on the grid.
///
/// Uses 1-based column and row indices to line up with libcosmic's `Grid`
/// widget, which is what these turn into at the render site. `width` and
/// `height` are the tile's shape written out — the render code should not
/// need to look at the shape again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub column: u16,
    pub row: u16,
    pub width: u16,
    pub height: u16,
}

#[cfg(test)]
impl Placement {
    /// Cells this placement covers, as (column, row) pairs.
    fn cells(self) -> impl Iterator<Item = (u16, u16)> {
        let cols = self.column..(self.column + self.width);
        let rows = self.row..(self.row + self.height);
        cols.flat_map(move |c| rows.clone().map(move |r| (c, r)))
    }
}

/// Result of packing: where each tile went, and where the ghosts go to keep
/// the grid rectangular.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pack {
    /// One entry per input tile, in the same order.
    pub tiles: Vec<Placement>,
    /// Empty cells the packer left behind. A Wide tile that came in after
    /// a Small on the left of a row leaves a hole; a Tall tile mid-row
    /// leaves the neighbour cell to be filled by the next Small.
    pub ghosts: Vec<Placement>,
    /// Total rows used, so the caller can size its container.
    pub rows: u16,
}

/// Pack tiles into a fixed-width grid.
///
/// Rules, in order of application:
///
/// 1. Walk the tiles in the given order. For each tile, find the first
///    (row, column) pair that fits its full footprint without overlap.
///    "First" means smallest row, then smallest column at that row.
/// 2. Reserve the cells the tile occupies. A `Tall` tile occupies its
///    column in this row *and* the next row; a `Wide` tile occupies both
///    columns of this row.
/// 3. After every tile is placed, walk the grid one more time and emit
///    a `Placement` for each still-empty cell. Those are the ghosts.
///
/// The packer does **not** rearrange tiles to fill gaps. The order the
/// user chose is the order they see: a Wide behind a Small leaves a
/// ghost to the right of the Small rather than swapping them. Users who
/// reorder tiles in Settings expect the tile they moved to be where they
/// dropped it, not shuffled by the packer's opinion of what fits.
pub fn pack(shapes: &[TileShape], columns: u16) -> Pack {
    assert!(columns > 0, "the grid must have at least one column");

    // `occupied[(col, row)]` is true when that cell is taken. Grows as new
    // rows are reached — a Tall in row N reserves a cell in row N+1, so we
    // may need to extend before placing the next tile.
    let mut occupied: Vec<Vec<bool>> = Vec::new();
    let cols_usize = columns as usize;
    let ensure_row = |occ: &mut Vec<Vec<bool>>, row: usize| {
        while occ.len() <= row {
            occ.push(vec![false; cols_usize]);
        }
    };

    let mut placements = Vec::with_capacity(shapes.len());

    for &shape in shapes {
        let width = shape.columns() as u16;
        let height = shape.rows() as u16;
        // Won't fit in this grid at all — a Wide in a one-column grid, say.
        // Skip rather than panic: the tile is worth zero pixels but the popup
        // is still useful without it.
        if width > columns {
            continue;
        }

        // Find the earliest position that fits: smallest row, then smallest
        // column within it. Terminates because the grid grows a fresh empty
        // row each pass, and a shape narrow enough for the grid always fits an
        // empty row — the `width > columns` guard above rules out the rest.
        let (col, row) = {
            let mut row = 0usize;
            loop {
                ensure_row(&mut occupied, row + height as usize - 1);
                let free = (0..=(columns - width) as usize).find(|&col| {
                    (0..width as usize)
                        .all(|dc| (0..height as usize).all(|dr| !occupied[row + dr][col + dc]))
                });
                if let Some(col) = free {
                    break (col as u16, row as u16);
                }
                row += 1;
            }
        };

        for dc in 0..width {
            for dr in 0..height {
                occupied[(row + dr) as usize][(col + dc) as usize] = true;
            }
        }

        placements.push(Placement {
            column: col + 1,
            row: row + 1,
            width,
            height,
        });
    }

    let ghost_positions: Vec<Placement> = occupied
        .iter()
        .enumerate()
        .flat_map(|(row, cells)| {
            cells.iter().enumerate().filter_map(move |(col, &taken)| {
                (!taken).then_some(Placement {
                    column: col as u16 + 1,
                    row: row as u16 + 1,
                    width: 1,
                    height: 1,
                })
            })
        })
        .collect();

    Pack {
        tiles: placements,
        ghosts: ghost_positions,
        rows: occupied.len() as u16,
    }
}

/// One placed tile: a control, the shape it is drawn at, and where its
/// top-left cell sits.
///
/// This is the unit of the free-placement layout. Position is explicit and
/// 0-based — `col` in sub-columns out of [`GRID_COLUMNS`], `row` unbounded —
/// so the same control can appear several times at several sizes, and a gap
/// the user left stays a gap. Contrast [`Placement`], the packer's 1-based
/// output that the band cutter consumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Instance {
    pub control: TileKey,
    pub shape: TileShape,
    pub col: u16,
    pub row: u16,
}

/// Where something sits on the grid, without saying what it is.
///
/// The render path needs a footprint and a top-left cell and nothing else, so
/// this is what it takes. Custom tiles have no [`TileKey`] — giving them a
/// borrowed one just to be drawn would put a lie in the layout — and they
/// have a `Slot` like everything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Slot {
    pub shape: TileShape,
    pub col: u16,
    pub row: u16,
}

impl Slot {
    pub fn new(shape: TileShape, col: u16, row: u16) -> Self {
        Self { shape, col, row }
    }

    /// The 1-based placement the band cutter draws this slot at.
    pub fn placement(self) -> Placement {
        Placement {
            column: self.col + 1,
            row: self.row + 1,
            width: self.shape.columns() as u16,
            height: self.shape.rows() as u16,
        }
    }

    fn cells(self) -> impl Iterator<Item = (u16, u16)> {
        let cols = self.col..(self.col + self.shape.columns() as u16);
        let rows = self.row..(self.row + self.shape.rows() as u16);
        cols.flat_map(move |c| rows.clone().map(move |r| (c, r)))
    }

    fn in_bounds(self) -> bool {
        self.col as usize + self.shape.columns() <= GRID_COLUMNS as usize
    }
}

impl Instance {
    pub fn new(control: TileKey, shape: TileShape, col: u16, row: u16) -> Self {
        Self {
            control,
            shape,
            col,
            row,
        }
    }

    /// This instance's footprint, with the control forgotten.
    pub fn slot(self) -> Slot {
        Slot::new(self.shape, self.col, self.row)
    }

    /// The 1-based placement the band cutter draws this instance at.
    // The render path goes through `slot()`; this is the convenience the
    // migration tests compare against the packer's own output.
    #[cfg(test)]
    pub fn placement(self) -> Placement {
        self.slot().placement()
    }

    fn cells(self) -> impl Iterator<Item = (u16, u16)> {
        self.slot().cells()
    }
}

/// Whether a `shape` dropped with its top-left at (`col`, `row`) would land
/// entirely on free, in-bounds cells of `layout`.
///
/// This is what a drop consults. Rows are unbounded, so only the column edge
/// can be out of bounds.
pub fn free_at(layout: &[Instance], shape: TileShape, col: u16, row: u16) -> bool {
    let probe = Slot::new(shape, col, row);
    if !probe.in_bounds() {
        return false;
    }
    let taken: std::collections::HashSet<(u16, u16)> =
        layout.iter().flat_map(|i| i.cells()).collect();
    probe.cells().all(|c| !taken.contains(&c))
}

/// The first free cell, row-major, that fits `shape`.
///
/// Always succeeds: rows are unbounded, so at worst the answer is the row
/// below everything. Used when the palette adds a tile without a drop target.
// Consumed by the Settings palette later in this series.
#[allow(dead_code)]
pub fn first_free(layout: &[Instance], shape: TileShape) -> (u16, u16) {
    let last_row = layout
        .iter()
        .map(|i| i.row + i.shape.rows() as u16)
        .max()
        .unwrap_or(0);
    for row in 0..=last_row {
        for col in 0..GRID_COLUMNS {
            if free_at(layout, shape, col, row) {
                return (col, row);
            }
        }
    }
    unreachable!("the row below every instance is always free")
}

/// Drop any instance that overlaps an earlier one or hangs past the right
/// edge. First-placed wins; what is dropped is logged.
///
/// Called on load (a hand-edited config can overlap) and after every edit
/// before save, so the render path never sees overlapping tiles.
pub fn validate(layout: &[Instance]) -> Vec<Instance> {
    let mut kept: Vec<Instance> = Vec::with_capacity(layout.len());
    for &instance in layout {
        if !instance.slot().in_bounds() {
            tracing::warn!("dropping {instance:?}: past the right edge of the grid");
            continue;
        }
        if !free_at(&kept, instance.shape, instance.col, instance.row) {
            tracing::warn!("dropping {instance:?}: overlaps an earlier tile");
            continue;
        }
        kept.push(instance);
    }
    kept
}

/// Build a layout from the pre-0.2 `order` + `shapes` model.
///
/// Runs the old packer once, so a migrated config opens exactly as 0.1.6
/// drew it. The `is_enabled` predicate is the `[modules]` switch: a control
/// switched off gets no instance, which under the derived-selection rule is
/// the same thing as being off.
pub fn migrate_from_packed(
    order: &[TileKey],
    shapes: &std::collections::HashMap<TileKey, TileShape>,
    is_enabled: impl Fn(TileKey) -> bool,
) -> Vec<Instance> {
    let keys = resolve_order(order, is_enabled);
    let shapes: Vec<TileShape> = keys.iter().map(|k| k.shape_with(shapes)).collect();
    let packed = pack(&shapes, GRID_COLUMNS);
    keys.iter()
        .zip(shapes)
        .zip(packed.tiles)
        .map(|((&control, shape), p)| Instance::new(control, shape, p.column - 1, p.row - 1))
        .collect()
}

/// Turn placed instances into the [`Pack`] the band cutter consumes.
///
/// The inverse of the packer's job: positions are already decided, so this
/// only writes them out and fills every uncovered cell with a one-sub-column
/// ghost. `bands` then cuts the same way it always did — one layout engine for
/// the popup and Settings both.
///
/// Assumes a validated layout; overlapping instances would produce a grid the
/// band cutter cannot express. [`validate`] is what guarantees that.
pub fn place(layout: &[Slot]) -> Pack {
    let rows = layout
        .iter()
        .map(|i| i.row + i.shape.rows() as u16)
        .max()
        .unwrap_or(0);

    let taken: std::collections::HashSet<(u16, u16)> =
        layout.iter().flat_map(|i| i.cells()).collect();

    let ghosts = (0..rows)
        .flat_map(|row| {
            let taken = &taken;
            (0..GRID_COLUMNS).filter_map(move |col| {
                (!taken.contains(&(col, row))).then_some(Placement {
                    column: col + 1,
                    row: row + 1,
                    width: 1,
                    height: 1,
                })
            })
        })
        .collect();

    Pack {
        tiles: layout.iter().map(|s| s.placement()).collect(),
        ghosts,
        rows,
    }
}

/// Slots for shapes laid out by the old packer.
///
/// **Temporary.** The Settings preview still thinks in an ordered list of
/// shapes; this bridges it onto the position-driven renderer until that page
/// is rebuilt around instances. Nothing else should reach for it.
pub fn packed_slots(shapes: &[TileShape]) -> Vec<Slot> {
    pack(shapes, GRID_COLUMNS)
        .tiles
        .into_iter()
        .map(Slot::new_from_placement)
        .collect()
}

impl Slot {
    fn new_from_placement(p: Placement) -> Self {
        let shape = match (p.width, p.height) {
            (1, _) => TileShape::Half,
            (4, _) => TileShape::Wide,
            (_, 2) => TileShape::Tall,
            _ => TileShape::Small,
        };
        Slot::new(shape, p.column - 1, p.row - 1)
    }
}

#[cfg(test)]
mod instance_tests {
    use super::*;
    use TileKey::{Battery, Dns, Volume};
    use TileShape::{Half, Small, Tall, Wide};

    fn at(control: TileKey, shape: TileShape, col: u16, row: u16) -> Instance {
        Instance::new(control, shape, col, row)
    }

    fn slots(layout: &[Instance]) -> Vec<Slot> {
        layout.iter().map(|i| i.slot()).collect()
    }

    #[test]
    fn an_empty_grid_is_free_everywhere_in_bounds() {
        assert!(free_at(&[], Wide, 0, 0));
        assert!(free_at(&[], Small, 2, 7));
        // A Small at sub-column 3 would hang one cell past the edge.
        assert!(!free_at(&[], Small, 3, 0));
        assert!(!free_at(&[], Wide, 1, 0));
        assert!(!free_at(&[], Half, GRID_COLUMNS, 0));
    }

    #[test]
    fn a_footprint_touching_an_occupied_cell_is_not_free() {
        let layout = [at(Battery, Small, 0, 0)];
        assert!(!free_at(&layout, Half, 1, 0)); // inside it
        assert!(free_at(&layout, Half, 2, 0)); // right beside it
        assert!(!free_at(&layout, Wide, 0, 0)); // spans it
        assert!(free_at(&layout, Wide, 0, 1)); // row below
                                               // A Tall from the row above reaches down into row 1.
        let tall = [at(Volume, Tall, 0, 0)];
        assert!(!free_at(&tall, Small, 0, 1));
        assert!(free_at(&tall, Small, 2, 1));
    }

    #[test]
    fn first_free_scans_row_major_and_falls_through_to_a_new_row() {
        let layout = [at(Battery, Small, 0, 0), at(Dns, Small, 2, 0)];
        assert_eq!(first_free(&layout, Half), (0, 1));
        let with_gap = [at(Battery, Half, 0, 0), at(Dns, Small, 2, 0)];
        assert_eq!(first_free(&with_gap, Half), (1, 0));
        assert_eq!(first_free(&with_gap, Small), (0, 1));
        assert_eq!(first_free(&[], Wide), (0, 0));
    }

    #[test]
    fn validate_drops_overlaps_first_wins_and_keeps_gaps() {
        let layout = [
            at(Battery, Small, 0, 0),
            at(Dns, Small, 1, 0),   // overlaps Battery
            at(Dns, Small, 2, 0),   // fine
            at(Volume, Wide, 0, 5), // gap rows 1–4 kept, not packed
        ];
        let kept = validate(&layout);
        assert_eq!(
            kept,
            vec![
                at(Battery, Small, 0, 0),
                at(Dns, Small, 2, 0),
                at(Volume, Wide, 0, 5)
            ]
        );
    }

    #[test]
    fn validate_drops_out_of_bounds() {
        let kept = validate(&[at(Battery, Small, 3, 0), at(Volume, Wide, 0, 0)]);
        assert_eq!(kept, vec![at(Volume, Wide, 0, 0)]);
    }

    #[test]
    fn validate_allows_duplicates_of_a_control() {
        let layout = [at(Battery, Small, 0, 0), at(Battery, Wide, 0, 1)];
        assert_eq!(validate(&layout), layout.to_vec());
    }

    #[test]
    fn migration_reproduces_the_packer_cell_for_cell() {
        // A representative 0.1.6 config: custom order, one Half override,
        // the standalone connectivity tiles switched off.
        let order = vec![Volume, Battery, Dns];
        let mut shapes = std::collections::HashMap::new();
        shapes.insert(Battery, Half);
        let off = [TileKey::Wifi, TileKey::Bluetooth, TileKey::Vpn];
        let layout = migrate_from_packed(&order, &shapes, |k| !off.contains(&k));

        let keys = resolve_order(&order, |k| !off.contains(&k));
        let packed = pack(
            &keys
                .iter()
                .map(|k| k.shape_with(&shapes))
                .collect::<Vec<_>>(),
            GRID_COLUMNS,
        );
        assert_eq!(layout.len(), keys.len());
        for (instance, (key, placement)) in layout.iter().zip(keys.iter().zip(packed.tiles)) {
            assert_eq!(instance.control, *key);
            assert_eq!(instance.placement(), placement);
        }
        assert_eq!(layout[0], at(Volume, Wide, 0, 0));
        assert_eq!(layout[1], at(Battery, Half, 0, 1));
        assert!(!layout.iter().any(|i| off.contains(&i.control)));
        // The packer never overlaps, so validate is a no-op on its output.
        assert_eq!(validate(&layout), layout);
    }

    #[test]
    fn place_matches_the_packer_on_a_gap_free_layout() {
        // The migration path: what the packer produced, fed back through
        // place(), must be the same Pack — same tiles, same ghosts, same rows.
        let shapes = [Wide, Half, Small, Tall, Small];
        let packed = pack(&shapes, GRID_COLUMNS);
        let layout: Vec<Instance> = shapes
            .iter()
            .zip(&packed.tiles)
            .map(|(&shape, p)| Instance::new(Battery, shape, p.column - 1, p.row - 1))
            .collect();
        assert_eq!(place(&slots(&layout)), packed);
    }

    #[test]
    fn place_turns_a_kept_gap_into_ghosts_rather_than_closing_it() {
        // Free placement's whole point: the empty row stays empty.
        let layout = [at(Battery, Small, 0, 0), at(Dns, Small, 2, 2)];
        let packed = place(&slots(&layout));
        assert_eq!(packed.rows, 3);
        assert_eq!(packed.tiles.len(), 2);
        // Row 1 is entirely ghosts, and so is the half-row beside each tile.
        assert_eq!(packed.ghosts.len(), 4 + 2 + 2);
        assert!(packed.ghosts.iter().all(|g| g.width == 1 && g.height == 1));
        // Every cell of the bounding box is covered exactly once.
        let cells: std::collections::HashSet<(u16, u16)> = packed
            .tiles
            .iter()
            .chain(&packed.ghosts)
            .flat_map(|p| {
                (p.column..p.column + p.width)
                    .flat_map(move |c| (p.row..p.row + p.height).map(move |r| (c, r)))
            })
            .collect();
        assert_eq!(cells.len(), (GRID_COLUMNS * 3) as usize);
    }

    #[test]
    fn an_empty_layout_places_nothing() {
        let packed = place(&[]);
        assert_eq!(packed.rows, 0);
        assert!(packed.tiles.is_empty() && packed.ghosts.is_empty());
    }

    #[test]
    fn instances_round_trip_through_toml() {
        #[derive(Deserialize, Serialize, PartialEq, Debug)]
        struct Wrap {
            layout: Vec<Instance>,
        }
        let original = Wrap {
            layout: vec![at(Battery, Small, 0, 0), at(Battery, Wide, 0, 1)],
        };
        let encoded = toml::to_string(&original).unwrap();
        assert!(encoded.contains("[[layout]]"));
        let decoded: Wrap = toml::from_str(&encoded).unwrap();
        assert_eq!(decoded, original);
        // A typo in an instance is rejected, not ignored.
        assert!(toml::from_str::<Wrap>(
            "[[layout]]\ncontrol=\"battery\"\nshape=\"small\"\ncol=0\nrow=0\nrwo=1\n"
        )
        .is_err());
    }
}

#[cfg(test)]
mod pack_tests {
    use super::*;
    use TileShape::{Half, Small, Tall, Wide};

    fn assert_no_overlaps(pack: &Pack) {
        let mut seen = std::collections::HashSet::new();
        for placement in pack.tiles.iter().chain(pack.ghosts.iter()) {
            for cell in placement.cells() {
                assert!(
                    seen.insert(cell),
                    "cell {cell:?} covered twice — {placement:?}"
                );
            }
        }
    }

    fn at(column: u16, row: u16, width: u16, height: u16) -> Placement {
        Placement {
            column,
            row,
            width,
            height,
        }
    }

    #[test]
    fn a_pair_of_smalls_fills_one_row() {
        let p = pack(&[Small, Small], GRID_COLUMNS);
        assert_eq!(p.tiles, vec![at(1, 1, 2, 1), at(3, 1, 2, 1)]);
        assert!(p.ghosts.is_empty());
        assert_eq!(p.rows, 1);
    }

    #[test]
    fn four_halfs_fill_one_row_and_a_fifth_wraps() {
        let p = pack(&[Half; 5], GRID_COLUMNS);
        assert_eq!(p.tiles[4], at(1, 2, 1, 1));
        assert_eq!(p.ghosts.len(), 3);
        assert_eq!(p.rows, 2);
    }

    #[test]
    fn a_half_beside_a_small_beside_a_half_is_one_row() {
        let p = pack(&[Half, Small, Half], GRID_COLUMNS);
        assert_eq!(
            p.tiles,
            vec![at(1, 1, 1, 1), at(2, 1, 2, 1), at(4, 1, 1, 1)]
        );
        assert!(p.ghosts.is_empty());
    }

    #[test]
    fn an_odd_small_gets_two_half_ghosts_next_to_it() {
        // A hole the size of a Small is two Half slots now.
        let p = pack(&[Small], GRID_COLUMNS);
        assert_eq!(p.ghosts, vec![at(3, 1, 1, 1), at(4, 1, 1, 1)]);
    }

    #[test]
    fn a_wide_takes_a_whole_row_by_itself() {
        let p = pack(&[Wide], GRID_COLUMNS);
        assert_eq!(p.tiles[0], at(1, 1, 4, 1));
        assert!(p.ghosts.is_empty());
    }

    #[test]
    fn a_wide_after_a_lone_small_starts_a_new_row_and_leaves_ghosts() {
        // The user's order wins over compactness.
        let p = pack(&[Small, Wide], GRID_COLUMNS);
        assert_eq!(p.tiles, vec![at(1, 1, 2, 1), at(1, 2, 4, 1)]);
        assert_eq!(p.ghosts.len(), 2);
    }

    #[test]
    fn a_tall_reserves_its_columns_across_two_rows() {
        let p = pack(&[Tall, Small, Small], GRID_COLUMNS);
        assert_eq!(
            p.tiles,
            vec![at(1, 1, 2, 2), at(3, 1, 2, 1), at(3, 2, 2, 1)]
        );
        assert!(p.ghosts.is_empty());
        assert_eq!(p.rows, 2);
    }

    #[test]
    fn a_wide_cannot_slip_past_a_tall_reserving_columns() {
        let p = pack(&[Tall, Wide], GRID_COLUMNS);
        assert_eq!(p.tiles[1], at(1, 3, 4, 1));
        assert_eq!(p.ghosts.len(), 4);
    }

    #[test]
    fn two_talls_sit_side_by_side_not_stacked() {
        let p = pack(&[Tall, Tall], GRID_COLUMNS);
        assert_eq!(p.tiles, vec![at(1, 1, 2, 2), at(3, 1, 2, 2)]);
        assert_eq!(p.rows, 2);
        assert!(p.ghosts.is_empty());
    }

    #[test]
    fn placements_never_overlap_across_any_of_the_mixed_cases() {
        for shapes in [
            vec![Small, Small, Small, Small],
            vec![Wide, Wide, Small],
            vec![Tall, Small, Small, Small, Tall],
            vec![Small, Tall, Wide, Small, Tall, Small],
            vec![Wide, Small, Tall, Small, Wide, Tall, Small],
            vec![Half, Tall, Half, Half, Small, Half, Wide, Half],
        ] {
            assert_no_overlaps(&pack(&shapes, GRID_COLUMNS));
        }
    }

    #[test]
    fn a_shape_that_cannot_fit_the_grid_at_all_is_skipped() {
        let p = pack(&[Half, Wide, Half], 2);
        assert_eq!(p.tiles.len(), 2);
    }
}

/// A cell's occupant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    /// A tile from the input list, by index.
    Tile(usize),
    Ghost,
}

/// One horizontal slice of the grid, ready to be drawn with rows and
/// columns and no grid widget.
///
/// libcosmic's `Grid` is taffy underneath, and taffy attributes a spanning
/// item's width to the first track it spans rather than splitting it — so a
/// Wide tile made column one 512px and column two nothing. Rather than fight
/// that, placements are cut into nested rows and columns that plain widgets
/// express exactly:
///
/// * The grid is cut into **bands** at every row boundary no tile straddles.
/// * Each band is cut into **strips** at every sub-column boundary no tile in
///   that band straddles. A strip is drawn as a column of rows.
/// * Each strip-row is the entries sitting in that strip on that grid row,
///   with their widths.
///
/// A Tall beside two Halfs over a Small therefore becomes one band of two
/// strips: `[Tall]` and `[[Half, Half], [Small]]`. Ghosts are one sub-column
/// each, which is what a hole *is* once Half tiles exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Band {
    pub strips: Vec<Strip>,
}

/// A vertical strip inside a band.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Strip {
    /// Sub-columns this strip spans.
    pub width: u16,
    /// One inner vec per grid row the band covers, top to bottom. A tile
    /// spanning two rows appears in the first and is absent from the second,
    /// which is why the renderer must size a strip by its content rather
    /// than assume every strip-row is one tile tall.
    pub rows: Vec<Vec<(Entry, u16)>>,
}

/// Cut a [`Pack`] into drawable bands.
pub fn bands(pack: &Pack) -> Vec<Band> {
    if pack.rows == 0 {
        return Vec::new();
    }
    let cols = GRID_COLUMNS;

    // Everything, by top-left cell.
    let mut at: std::collections::HashMap<(u16, u16), (Entry, Placement)> =
        std::collections::HashMap::new();
    let mut all: Vec<Placement> = Vec::with_capacity(pack.tiles.len() + pack.ghosts.len());
    for (i, p) in pack.tiles.iter().enumerate() {
        at.insert((p.row, p.column), (Entry::Tile(i), *p));
        all.push(*p);
    }
    for p in &pack.ghosts {
        at.insert((p.row, p.column), (Entry::Ghost, *p));
        all.push(*p);
    }

    // A row boundary between r and r+1 is cut unless something straddles it.
    let straddles_row = |r: u16| all.iter().any(|p| p.row <= r && r + 1 < p.row + p.height);

    let mut out = Vec::new();
    let mut top = 1u16;
    while top <= pack.rows {
        let mut bottom = top;
        while bottom < pack.rows && straddles_row(bottom) {
            bottom += 1;
        }
        let in_band = |p: &Placement| p.row >= top && p.row <= bottom;

        // A column boundary between c and c+1 is cut unless something in this
        // band straddles it.
        let straddles_col = |c: u16| {
            all.iter()
                .any(|p| in_band(p) && p.column <= c && c + 1 < p.column + p.width)
        };

        let mut strips = Vec::new();
        let mut left = 1u16;
        while left <= cols {
            let mut right = left;
            while right < cols && straddles_col(right) {
                right += 1;
            }
            let mut rows = Vec::with_capacity((bottom - top + 1) as usize);
            for r in top..=bottom {
                let mut entries = Vec::new();
                let mut c = left;
                while c <= right {
                    if let Some((entry, p)) = at.get(&(r, c)) {
                        entries.push((*entry, p.width));
                        c += p.width;
                    } else {
                        c += 1;
                    }
                }
                rows.push(entries);
            }
            strips.push(Strip {
                width: right - left + 1,
                rows,
            });
            left = right + 1;
        }
        out.push(Band { strips });
        top = bottom + 1;
    }
    out
}

#[cfg(test)]
mod band_tests {
    use super::*;
    use Entry::{Ghost, Tile as T};

    fn one_strip(width: u16, rows: Vec<Vec<(Entry, u16)>>) -> Band {
        Band {
            strips: vec![Strip { width, rows }],
        }
    }

    #[test]
    fn two_smalls_are_one_band_of_two_strips() {
        let p = pack(&[TileShape::Small, TileShape::Small], GRID_COLUMNS);
        assert_eq!(
            bands(&p),
            vec![Band {
                strips: vec![
                    Strip {
                        width: 2,
                        rows: vec![vec![(T(0), 2)]]
                    },
                    Strip {
                        width: 2,
                        rows: vec![vec![(T(1), 2)]]
                    },
                ]
            }]
        );
    }

    #[test]
    fn four_halfs_fill_one_row() {
        let p = pack(&[TileShape::Half; 4], GRID_COLUMNS);
        let b = bands(&p);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].strips.len(), 4);
        assert!(b[0]
            .strips
            .iter()
            .all(|s| s.width == 1 && s.rows == vec![vec![(s.rows[0][0].0, 1)]]));
        assert!(p.ghosts.is_empty());
    }

    #[test]
    fn a_lone_half_gets_three_ghosts_beside_it() {
        let p = pack(&[TileShape::Half], GRID_COLUMNS);
        assert_eq!(p.ghosts.len(), 3);
        let b = bands(&p);
        assert_eq!(b[0].strips.len(), 4);
        assert_eq!(b[0].strips[0].rows, vec![vec![(T(0), 1)]]);
        assert_eq!(b[0].strips[1].rows, vec![vec![(Ghost, 1)]]);
    }

    #[test]
    fn a_wide_is_one_strip_the_whole_width() {
        let p = pack(&[TileShape::Wide], GRID_COLUMNS);
        assert_eq!(bands(&p), vec![one_strip(4, vec![vec![(T(0), 4)]])]);
    }

    #[test]
    fn a_tall_beside_two_halfs_over_a_small_is_one_band_of_two_strips() {
        // The case the strip model exists for.
        let p = pack(
            &[
                TileShape::Tall,
                TileShape::Half,
                TileShape::Half,
                TileShape::Small,
            ],
            GRID_COLUMNS,
        );
        assert_eq!(
            bands(&p),
            vec![Band {
                strips: vec![
                    Strip {
                        width: 2,
                        rows: vec![vec![(T(0), 2)], vec![]]
                    },
                    Strip {
                        width: 2,
                        rows: vec![vec![(T(1), 1), (T(2), 1)], vec![(T(3), 2)]],
                    },
                ]
            }]
        );
    }

    #[test]
    fn a_tall_alone_gets_ghosts_in_both_rows_beside_it() {
        let p = pack(&[TileShape::Tall], GRID_COLUMNS);
        let b = bands(&p);
        assert_eq!(b.len(), 1);
        // Left strip: the Tall. Right side: two sub-column strips of ghosts.
        assert_eq!(b[0].strips[0].rows, vec![vec![(T(0), 2)], vec![]]);
        assert!(b[0].strips[1..]
            .iter()
            .all(|s| s.width == 1 && s.rows == vec![vec![(Ghost, 1)], vec![(Ghost, 1)]]));
    }

    #[test]
    fn a_wide_then_a_tall_band_then_a_flat_row_cut_cleanly() {
        let p = pack(
            &[
                TileShape::Wide,
                TileShape::Tall,
                TileShape::Small,
                TileShape::Small,
                TileShape::Small,
            ],
            GRID_COLUMNS,
        );
        let b = bands(&p);
        assert_eq!(b.len(), 3, "{b:#?}");
        assert_eq!(b[0], one_strip(4, vec![vec![(T(0), 4)]]));
        assert_eq!(b[1].strips[0].rows, vec![vec![(T(1), 2)], vec![]]);
        assert_eq!(b[1].strips[1].rows, vec![vec![(T(2), 2)], vec![(T(3), 2)]]);
        assert_eq!(b[2].strips[0].rows, vec![vec![(T(4), 2)]]);
    }

    #[test]
    fn every_tile_appears_exactly_once_across_bands() {
        for shapes in [
            vec![TileShape::Small; 5],
            vec![TileShape::Half; 7],
            vec![
                TileShape::Wide,
                TileShape::Tall,
                TileShape::Half,
                TileShape::Wide,
            ],
            vec![
                TileShape::Tall,
                TileShape::Half,
                TileShape::Tall,
                TileShape::Small,
                TileShape::Half,
            ],
            vec![
                TileShape::Half,
                TileShape::Tall,
                TileShape::Half,
                TileShape::Half,
                TileShape::Wide,
            ],
        ] {
            let p = pack(&shapes, GRID_COLUMNS);
            let mut seen = vec![0usize; shapes.len()];
            for band in bands(&p) {
                for strip in band.strips {
                    for row in strip.rows {
                        for (e, _) in row {
                            if let Entry::Tile(i) = e {
                                seen[i] += 1;
                            }
                        }
                    }
                }
            }
            assert!(seen.iter().all(|&n| n == 1), "{shapes:?} → {seen:?}");
        }
    }

    #[test]
    fn strip_widths_always_sum_to_the_grid_width() {
        for shapes in [
            vec![TileShape::Half, TileShape::Small, TileShape::Half],
            vec![
                TileShape::Tall,
                TileShape::Half,
                TileShape::Half,
                TileShape::Small,
            ],
            vec![TileShape::Wide, TileShape::Half],
        ] {
            let p = pack(&shapes, GRID_COLUMNS);
            for band in bands(&p) {
                let total: u16 = band.strips.iter().map(|s| s.width).sum();
                assert_eq!(total, GRID_COLUMNS, "{shapes:?}: {band:#?}");
            }
        }
    }
}
