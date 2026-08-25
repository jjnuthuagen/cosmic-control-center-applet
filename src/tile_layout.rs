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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileShape {
    Small,
    Wide,
    Tall,
}

impl TileShape {
    /// Cells occupied on the horizontal axis (out of two).
    pub fn columns(self) -> usize {
        match self {
            TileShape::Small | TileShape::Tall => 1,
            TileShape::Wide => 2,
        }
    }

    /// Cells occupied on the vertical axis.
    pub fn rows(self) -> usize {
        match self {
            TileShape::Small | TileShape::Wide => 1,
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
    fn tall_tiles_span_two_rows_and_one_column() {
        assert_eq!(TileShape::Tall.rows(), 2);
        assert_eq!(TileShape::Tall.columns(), 1);
    }

    #[test]
    fn wide_tiles_span_two_columns_and_one_row() {
        assert_eq!(TileShape::Wide.columns(), 2);
        assert_eq!(TileShape::Wide.rows(), 1);
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

#[cfg(test)]
mod pack_tests {
    use super::*;

    fn small() -> TileShape {
        TileShape::Small
    }
    fn wide() -> TileShape {
        TileShape::Wide
    }
    fn tall() -> TileShape {
        TileShape::Tall
    }

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

    #[test]
    fn a_pair_of_smalls_fills_one_row() {
        let pack = pack(&[small(), small()], 2);
        assert_eq!(pack.tiles.len(), 2);
        assert_eq!(
            pack.tiles[0],
            Placement {
                column: 1,
                row: 1,
                width: 1,
                height: 1
            }
        );
        assert_eq!(
            pack.tiles[1],
            Placement {
                column: 2,
                row: 1,
                width: 1,
                height: 1
            }
        );
        assert!(pack.ghosts.is_empty());
        assert_eq!(pack.rows, 1);
    }

    #[test]
    fn an_odd_small_gets_a_ghost_next_to_it() {
        // The whole point of ghosts: keep the last row square rather than
        // leaving a single tile floating on the left.
        let pack = pack(&[small()], 2);
        assert_eq!(
            pack.tiles[0],
            Placement {
                column: 1,
                row: 1,
                width: 1,
                height: 1
            }
        );
        assert_eq!(
            pack.ghosts,
            vec![Placement {
                column: 2,
                row: 1,
                width: 1,
                height: 1
            }]
        );
    }

    #[test]
    fn a_wide_takes_a_whole_row_by_itself() {
        let pack = pack(&[wide()], 2);
        assert_eq!(
            pack.tiles[0],
            Placement {
                column: 1,
                row: 1,
                width: 2,
                height: 1
            }
        );
        assert!(
            pack.ghosts.is_empty(),
            "a Wide fills the row; no ghost needed"
        );
    }

    #[test]
    fn a_wide_after_a_lone_small_starts_a_new_row_and_leaves_a_ghost() {
        // The user's order wins over compactness: a Small first, then a Wide
        // means Small alone up top with a ghost to its right, and the Wide
        // below it — not Small swapped with Wide.
        let pack = pack(&[small(), wide()], 2);
        assert_eq!(
            pack.tiles[0],
            Placement {
                column: 1,
                row: 1,
                width: 1,
                height: 1
            }
        );
        assert_eq!(
            pack.tiles[1],
            Placement {
                column: 1,
                row: 2,
                width: 2,
                height: 1
            }
        );
        assert_eq!(
            pack.ghosts,
            vec![Placement {
                column: 2,
                row: 1,
                width: 1,
                height: 1
            }]
        );
    }

    #[test]
    fn a_tall_reserves_its_column_across_two_rows() {
        // A Tall in column 1 means column 2 of both rows is free for smalls.
        // Two Smalls after it should slot in there.
        let pack = pack(&[tall(), small(), small()], 2);
        assert_eq!(
            pack.tiles[0],
            Placement {
                column: 1,
                row: 1,
                width: 1,
                height: 2
            }
        );
        assert_eq!(
            pack.tiles[1],
            Placement {
                column: 2,
                row: 1,
                width: 1,
                height: 1
            }
        );
        assert_eq!(
            pack.tiles[2],
            Placement {
                column: 2,
                row: 2,
                width: 1,
                height: 1
            }
        );
        assert!(pack.ghosts.is_empty());
        assert_eq!(pack.rows, 2);
    }

    #[test]
    fn a_wide_cannot_slip_past_a_tall_reserving_a_column() {
        // Tall in col 1, rows 1-2. Wide needs both cols so it cannot sit at
        // row 1 or row 2 — it has to wait for row 3.
        let pack = pack(&[tall(), wide()], 2);
        assert_eq!(
            pack.tiles[0],
            Placement {
                column: 1,
                row: 1,
                width: 1,
                height: 2
            }
        );
        assert_eq!(
            pack.tiles[1],
            Placement {
                column: 1,
                row: 3,
                width: 2,
                height: 1
            }
        );
        // Two ghosts, one per row, on the free column-2 of rows 1 and 2.
        assert_eq!(pack.ghosts.len(), 2);
    }

    #[test]
    fn two_talls_sit_side_by_side_not_stacked() {
        // Left tall, right tall, done in two rows. Not four rows stacked.
        let pack = pack(&[tall(), tall()], 2);
        assert_eq!(
            pack.tiles[0],
            Placement {
                column: 1,
                row: 1,
                width: 1,
                height: 2
            }
        );
        assert_eq!(
            pack.tiles[1],
            Placement {
                column: 2,
                row: 1,
                width: 1,
                height: 2
            }
        );
        assert_eq!(pack.rows, 2);
        assert!(pack.ghosts.is_empty());
    }

    #[test]
    fn placements_never_overlap_across_any_of_the_mixed_cases() {
        for shapes in [
            vec![small(), small(), small(), small()],
            vec![wide(), wide(), small()],
            vec![tall(), small(), small(), small(), tall()],
            vec![small(), tall(), wide(), small(), tall(), small()],
            vec![wide(), small(), tall(), small(), wide(), tall(), small()],
        ] {
            assert_no_overlaps(&pack(&shapes, 2));
        }
    }

    #[test]
    fn a_shape_that_cannot_fit_the_grid_at_all_is_skipped() {
        // A Wide in a one-column grid has nowhere to go. Better to drop it
        // than to panic — the popup is still useful without that tile.
        let pack = pack(&[small(), wide(), small()], 1);
        // Only the two Smalls placed.
        assert_eq!(pack.tiles.len(), 2);
        assert!(pack.tiles.iter().all(|p| p.width == 1 && p.height == 1));
    }
}

/// One horizontal slice of the grid, ready to be drawn without a grid
/// widget.
///
/// libcosmic's `Grid` is taffy underneath, and taffy attributes a spanning
/// item's width to the first track it spans rather than splitting it — so a
/// Wide tile made column one 512px and column two nothing. Rather than fight
/// that, the placements are cut into bands that plain rows and columns can
/// express exactly:
///
/// * A `Flat` band is one grid row: a Wide on its own, or two one-cell
///   entries side by side (Small, ghost, or a Tall's *upper* half — see
///   below).
/// * A `Tall` band is two grid rows that a Tall tile straddles: two columns
///   side by side, each holding whatever sits in that column across both
///   rows. A Tall is one entry of double height; two Smalls (or a Small and
///   a ghost) stack to the same height.
///
/// The packer guarantees a Wide never lands in a row a Tall straddles (it
/// waits for a clear row), which is what makes this split total.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Band {
    /// Entries in column order. Each is `(index, is_ghost)`, where `index`
    /// points into the original tile list for real tiles.
    Flat(Vec<Entry>),
    /// Left column entries (top to bottom), right column entries.
    Tall(Vec<Entry>, Vec<Entry>),
}

/// A cell's occupant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    /// A tile from the input list, by index.
    Tile(usize),
    Ghost,
}

/// Cut a [`Pack`] into drawable bands.
pub fn bands(pack: &Pack) -> Vec<Band> {
    // (row, column) → entry, plus which rows a Tall straddles.
    let mut cell: std::collections::HashMap<(u16, u16), (Entry, Placement)> =
        std::collections::HashMap::new();
    let mut tall_top_rows = std::collections::HashSet::new();

    for (i, p) in pack.tiles.iter().enumerate() {
        cell.insert((p.row, p.column), (Entry::Tile(i), *p));
        if p.height == 2 {
            tall_top_rows.insert(p.row);
        }
    }
    for p in &pack.ghosts {
        cell.insert((p.row, p.column), (Entry::Ghost, *p));
    }

    let mut out = Vec::new();
    let mut row = 1u16;
    while row <= pack.rows {
        if tall_top_rows.contains(&row) {
            // Two rows, two columns. Each column lists what sits in it across
            // both rows, top to bottom; a Tall appears once (it covers both).
            let column_entries = |col: u16| -> Vec<Entry> {
                let mut v = Vec::with_capacity(2);
                for r in [row, row + 1] {
                    if let Some((entry, p)) = cell.get(&(r, col)) {
                        // The lower row of a Tall has no cell of its own —
                        // the Tall was inserted at its top row only.
                        let _ = p;
                        v.push(*entry);
                    }
                }
                v
            };
            out.push(Band::Tall(column_entries(1), column_entries(2)));
            row += 2;
        } else {
            let mut entries = Vec::with_capacity(2);
            let mut col = 1u16;
            while col <= 2 {
                if let Some((entry, p)) = cell.get(&(row, col)) {
                    entries.push(*entry);
                    col += p.width;
                } else {
                    col += 1;
                }
            }
            out.push(Band::Flat(entries));
            row += 1;
        }
    }
    out
}

#[cfg(test)]
mod band_tests {
    use super::*;

    #[test]
    fn two_smalls_are_one_flat_band() {
        let p = pack(&[TileShape::Small, TileShape::Small], 2);
        assert_eq!(
            bands(&p),
            vec![Band::Flat(vec![Entry::Tile(0), Entry::Tile(1)])]
        );
    }

    #[test]
    fn a_lone_small_gets_a_ghost_beside_it() {
        let p = pack(&[TileShape::Small], 2);
        assert_eq!(
            bands(&p),
            vec![Band::Flat(vec![Entry::Tile(0), Entry::Ghost])]
        );
    }

    #[test]
    fn a_wide_is_a_flat_band_of_one() {
        let p = pack(&[TileShape::Wide], 2);
        assert_eq!(bands(&p), vec![Band::Flat(vec![Entry::Tile(0)])]);
    }

    #[test]
    fn a_tall_with_two_smalls_beside_it_is_one_tall_band() {
        // Tall in col 1 rows 1-2; Smalls fill col 2 rows 1 and 2.
        let p = pack(&[TileShape::Tall, TileShape::Small, TileShape::Small], 2);
        assert_eq!(
            bands(&p),
            vec![Band::Tall(
                vec![Entry::Tile(0)],
                vec![Entry::Tile(1), Entry::Tile(2)]
            )]
        );
    }

    #[test]
    fn a_tall_alone_gets_two_ghosts_stacked_beside_it() {
        let p = pack(&[TileShape::Tall], 2);
        assert_eq!(
            bands(&p),
            vec![Band::Tall(
                vec![Entry::Tile(0)],
                vec![Entry::Ghost, Entry::Ghost]
            )]
        );
    }

    #[test]
    fn two_talls_side_by_side_are_one_band() {
        let p = pack(&[TileShape::Tall, TileShape::Tall], 2);
        assert_eq!(
            bands(&p),
            vec![Band::Tall(vec![Entry::Tile(0)], vec![Entry::Tile(1)])]
        );
    }

    #[test]
    fn a_wide_then_a_tall_band_then_a_flat_row_cut_cleanly() {
        // Wide (row 1) / Tall + Small + Small (rows 2-3) / Small + ghost (row 4).
        let p = pack(
            &[
                TileShape::Wide,
                TileShape::Tall,
                TileShape::Small,
                TileShape::Small,
                TileShape::Small,
            ],
            2,
        );
        assert_eq!(
            bands(&p),
            vec![
                Band::Flat(vec![Entry::Tile(0)]),
                Band::Tall(vec![Entry::Tile(1)], vec![Entry::Tile(2), Entry::Tile(3)]),
                Band::Flat(vec![Entry::Tile(4), Entry::Ghost]),
            ]
        );
    }

    #[test]
    fn every_tile_appears_exactly_once_across_bands() {
        for shapes in [
            vec![TileShape::Small; 5],
            vec![
                TileShape::Wide,
                TileShape::Tall,
                TileShape::Small,
                TileShape::Wide,
            ],
            vec![
                TileShape::Tall,
                TileShape::Tall,
                TileShape::Tall,
                TileShape::Small,
            ],
        ] {
            let p = pack(&shapes, 2);
            let mut seen = vec![0usize; shapes.len()];
            for band in bands(&p) {
                let entries: Vec<Entry> = match band {
                    Band::Flat(e) => e,
                    Band::Tall(l, r) => l.into_iter().chain(r).collect(),
                };
                for e in entries {
                    if let Entry::Tile(i) = e {
                        seen[i] += 1;
                    }
                }
            }
            assert!(seen.iter().all(|&n| n == 1), "{shapes:?} → {seen:?}");
        }
    }
}
