//! Region partition: groups same-level, same-class connected cells into
//! region objects that the starts, tech and tiberium phases consume.
//!
//! Order: reset -> seed scan (water/green-class cells) -> per-region
//! multi-pass expansion + water propagation -> fallback seeding of every
//! remaining cell. The flood fill is a LIFO stack walk with claim-at-enqueue;
//! small blobs of the plain-land class get dissolved or merged into a
//! neighbor instead of becoming regions.

use crate::map::rmg::grid::RmgGrid;
use crate::map::rmg::rng::RmgRng;
use crate::map::rmg::scratch::RmgScratch;
use crate::map::rmg::tiles::TileIds;

/// Pop-count threshold at or below which a plain-class blob is dissolved or
/// merged rather than registered.
const SMALL_BLOB_POPS: i32 = 0x4A;
/// Large-region threshold for the extra expansion pass.
const LARGE_REGION_CELLS: i32 = 8000;

/// One region object (the fields later phases read).
#[derive(Debug, Clone)]
pub struct RmgRegion {
    pub id: i32,
    /// Seed cell's level; expansion claims only level-matching cells.
    pub level: u8,
    /// Class flag: seed was water-ish or green (drives the expansion loop).
    pub active: bool,
    pub cell_count: i32,
    /// Per-region cell list; populated by the starts phase, not here.
    pub cells: Vec<(i16, i16)>,
    /// Start slots assigned by the starts phase.
    pub start_quota: i32,
    /// Tiberium field slots selected by the starts phase. `None` = the
    /// selector produced no list for this region (its region+0x00 stays null);
    /// `Some(vec)` = a list (possibly empty after the waypoints were removed).
    /// The tiberium driver treats the two differently — a `None` region is
    /// skipped before any RNG, a `Some` region runs the gem-anchor draw.
    pub field_slots: Option<Vec<(i16, i16)>>,
}

/// The region registry: creation-ordered objects plus the id counter.
#[derive(Debug, Default)]
pub struct Regions {
    pub list: Vec<RmgRegion>,
    pub id_counter: i32,
}

impl RmgRegion {
    /// A region produced by the inland/mountainous rebuild, which derives all
    /// of its fields from the flood rather than from the seed scan.
    pub fn rebuilt(id: i32, level: u8, active: bool, cell_count: i32) -> Self {
        Self {
            id,
            level,
            active,
            cell_count,
            cells: Vec::new(),
            start_quota: 0,
            field_slots: None,
        }
    }
}

pub struct RegionCtx<'a> {
    pub grid: &'a mut RmgGrid,
    pub scratch: &'a mut RmgScratch,
    pub ids: &'a TileIds,
    pub rng: &'a mut RmgRng,
    pub map_type: i32,
    /// Fallback level for dissolve writes when the adopted level reads 0xFF.
    pub default_level: u8,
}

/// Water-ish class half. This is the exact tile-only family predicate called
/// by the native seed scan, flood fill, and first-region dissolve path.
fn water_ish(ids: &TileIds, tile: i32) -> bool {
    ids.is_water_shore_or_waterfall(tile)
}

/// The region class bit: water-ish OR green membership.
fn region_class(ids: &TileIds, tile: i32) -> bool {
    water_ish(ids, tile) || ids.is_green_lat(tile)
}

/// Each registered region's cells, kept as they are claimed, so the
/// per-region passes visit only the region instead of every map cell: the
/// native collectors scan the whole map once per region, which grows with the
/// square of a VERA Huge or Giant map's area. [`boundary`] sorts the cells
/// back into native scan order, so every pass sees what the scan would give.
struct Members {
    cells: std::collections::HashMap<i32, Vec<(i32, i32)>>,
    /// Native scan position of each cell, by `y * width + x`.
    rank: Vec<u32>,
    width: usize,
}

impl Members {
    fn new(grid: &RmgGrid) -> Self {
        let width = grid.width();
        let mut rank = vec![u32::MAX; width * width];
        for (position, (x, y)) in grid.native_cells().enumerate() {
            rank[y as usize * width + x as usize] = position as u32;
        }
        Self {
            cells: std::collections::HashMap::new(),
            rank,
            width,
        }
    }

    fn add(&mut self, id: i32, cells: impl IntoIterator<Item = (i32, i32)>) {
        self.cells.entry(id).or_default().extend(cells);
    }

    /// `id`'s cells in native scan order.
    fn in_scan_order(&self, id: i32) -> Vec<(i32, i32)> {
        let mut cells = self.cells.get(&id).cloned().unwrap_or_default();
        cells.sort_unstable_by_key(|&(x, y)| self.rank[y as usize * self.width + x as usize]);
        cells
    }
}

pub fn run(ctx: &mut RegionCtx<'_>) -> Regions {
    let mut regions = Regions::default();
    let mut members = Members::new(ctx.grid);
    ctx.scratch.reset_region_ids();

    // Seed scan: linear scratch order, class cells only.
    let width = ctx.scratch.width();
    for index in 0..width * width {
        let record = ctx.scratch.cells()[index];
        let coord = (i32::from(record.x), i32::from(record.y));
        if record.region != -1 || coord == (0, 0) {
            continue;
        }
        let tile = ctx.grid.cell_native(coord.0, coord.1).tile;
        if region_class(ctx.ids, tile) {
            flood_fill(ctx, &mut regions, &mut members, coord);
        }
    }

    // Expansion over the snapshot of seeded regions.
    let mode34 = matches!(ctx.map_type, 3 | 4);
    let seeded = regions.list.len();
    for index in 0..seeded {
        if !regions.list[index].active {
            continue;
        }
        let passes =
            4 + i32::from(regions.list[index].cell_count > LARGE_REGION_CELLS) + i32::from(!mode34);
        let (id, level) = (regions.list[index].id, regions.list[index].level);
        let _ = expand(ctx, &mut members, id, level, passes);
        water_propagate(ctx, &mut members, id);
    }

    // Fallback: every remaining unassigned cell seeds a region.
    for index in 0..width * width {
        let record = ctx.scratch.cells()[index];
        let coord = (i32::from(record.x), i32::from(record.y));
        if record.region != -1 || coord == (0, 0) {
            continue;
        }
        flood_fill(ctx, &mut regions, &mut members, coord);
    }

    regions
}

/// The flood-fill constructor: LIFO stack, claim-at-enqueue, level+class
/// matched growth, small-blob dissolve/merge, and the registry append.
fn flood_fill(
    ctx: &mut RegionCtx<'_>,
    regions: &mut Regions,
    members: &mut Members,
    seed: (i32, i32),
) {
    let id = regions.id_counter;
    let seed_tile = ctx.grid.cell_native(seed.0, seed.1).tile;
    let seed_class = region_class(ctx.ids, seed_tile);
    let seed_level = ctx.grid.cell_native(seed.0, seed.1).level;

    let mut stack: Vec<(i32, i32)> = vec![seed];
    let mut pops = 0i32;
    // Each cell is pushed once (its stamp is claimed first), so the popped
    // cells are exactly the cells this fill gives `id`.
    let mut filled: Vec<(i32, i32)> = Vec::new();
    while let Some(coord) = stack.pop() {
        pops += 1;
        filled.push(coord);
        {
            let record = ctx.scratch.get_mut(coord.0, coord.1);
            record.stamp = id;
            record.region = id;
        }
        for dir in 0..8 {
            let (nx, ny) = RmgGrid::step(coord.0, coord.1, dir);
            if !ctx.scratch.in_diamond(nx, ny) {
                continue;
            }
            if ctx.scratch.get(nx, ny).region != -1 || ctx.scratch.get(nx, ny).stamp == id {
                continue;
            }
            // Claim the stamp before the level/class tests: rejected cells
            // stay stamped and are never re-examined by this fill.
            ctx.scratch.get_mut(nx, ny).stamp = id;
            let neighbor = *ctx.grid.cell_native(nx, ny);
            if neighbor.level != seed_level {
                continue;
            }
            if region_class(ctx.ids, neighbor.tile) == seed_class {
                stack.push((nx, ny));
            }
        }
    }

    // Small plain-class blobs are dissolved (first region) or merged.
    if pops <= SMALL_BLOB_POPS && !seed_class {
        if id == 0 {
            if dissolve_first_region(ctx) {
                return;
            }
        } else {
            let mut cand = (seed.0 - 1, seed.1);
            if !ctx.scratch.in_diamond(cand.0, cand.1) {
                cand = (seed.0, seed.1 - 1);
            }
            if ctx.scratch.in_diamond(cand.0, cand.1) {
                let adopt_id = ctx.scratch.get(cand.0, cand.1).region;
                let adopt_level = ctx.grid.cell_native(cand.0, cand.1).level;
                rewrite_cells(ctx, &filled, adopt_id, adopt_level);
                if adopt_id != -1 {
                    members.add(adopt_id, filled);
                }
                return;
            }
        }
    }

    // Register the region. The ctor's water-ish class flag is immediately
    // overwritten by the fill's own (water OR green) bit, so only the latter
    // is modeled.
    regions.id_counter += 1;
    let mut region = RmgRegion {
        id,
        level: seed_level,
        active: seed_class,
        cell_count: 0,
        cells: Vec::new(),
        start_quota: 0,
        field_slots: None,
    };
    // The native count scans for every `id` cell: this fill's, plus a blob an
    // earlier fill of the same unregistered id merged into itself. Every
    // in-diamond cell has a nonzero coordinate sum, so none is the (0, 0) the
    // scan skips.
    members.add(id, filled);
    region.cell_count = members.cells.get(&id).map_or(0, |cells| {
        cells
            .iter()
            .filter(|&&(x, y)| ctx.scratch.get(x, y).region == id)
            .count()
    }) as i32;
    regions.list.push(region);
}

/// First-region dissolve: pick a random border cell of blob 0, look for a
/// differently-leveled non-water neighbor, and if found dissolve the whole
/// blob back to unassigned clear land at that neighbor's level.
fn dissolve_first_region(ctx: &mut RegionCtx<'_>) -> bool {
    let width = ctx.scratch.width();
    let mut border: Vec<(i32, i32)> = Vec::new();
    for index in 0..width * width {
        let record = ctx.scratch.cells()[index];
        let coord = (i32::from(record.x), i32::from(record.y));
        if coord == (0, 0) || record.region != 0 {
            continue;
        }
        let is_border = (0..8).any(|dir| {
            let (nx, ny) = RmgGrid::step(coord.0, coord.1, dir);
            ctx.scratch.in_diamond(nx, ny) && ctx.scratch.get(nx, ny).region != 0
        });
        if is_border {
            border.push(coord);
        }
    }
    if border.is_empty() {
        return false;
    }
    let pick = border[ctx.rng.uniform(0, border.len() as i32 - 1) as usize];
    let pick_level = ctx.grid.cell_native(pick.0, pick.1).level;

    for dir in 0..8 {
        let (nx, ny) = RmgGrid::step(pick.0, pick.1, dir);
        if !ctx.scratch.in_diamond(nx, ny) {
            continue;
        }
        let neighbor = *ctx.grid.cell_native(nx, ny);
        if neighbor.level != pick_level && !water_ish(ctx.ids, neighbor.tile) {
            rewrite_blob(ctx, 0, -1, neighbor.level);
            return true;
        }
    }
    false
}

/// Rewrite every cell of `blob_id` to `adopt_id` / clear land at the adopted
/// level (0xFF adopts the default), clearing the water flag.
fn rewrite_blob(ctx: &mut RegionCtx<'_>, blob_id: i32, adopt_id: i32, adopt_level: u8) {
    let cells: Vec<_> = ctx
        .grid
        .native_cells()
        .filter(|&(x, y)| ctx.scratch.get(x, y).region == blob_id)
        .collect();
    rewrite_cells(ctx, &cells, adopt_id, adopt_level);
}

/// [`rewrite_blob`] for a blob whose cells are known. Each cell's rewrite
/// stands alone, so their order does not matter.
fn rewrite_cells(ctx: &mut RegionCtx<'_>, cells: &[(i32, i32)], adopt_id: i32, adopt_level: u8) {
    let level = if adopt_level != 0xFF {
        adopt_level
    } else {
        ctx.default_level
    };
    for &(x, y) in cells {
        {
            let record = ctx.scratch.get_mut(x, y);
            record.region = adopt_id;
            record.water_region = false;
        }
        let cell = ctx.grid.get_mut(x, y).expect("native cell");
        cell.tile = 0;
        cell.sub_tile = 0;
        cell.level = level;
    }
}

/// Multi-pass ring expansion: pass 1 seeds from the full region boundary,
/// later passes from the previous pass's claims. Returns false on the abort
/// conditions (foreign region contact, or an unclaimed level-matching
/// neighbor failing the clear/green class test).
fn expand(ctx: &mut RegionCtx<'_>, members: &mut Members, id: i32, level: u8, passes: i32) -> bool {
    let mut frontier = boundary(ctx, members, id);
    for _ in 0..passes {
        let mut next: Vec<(i32, i32)> = Vec::new();
        for &(x, y) in &frontier {
            for dir in 0..8 {
                let (nx, ny) = RmgGrid::step(x, y, dir);
                if !ctx.scratch.in_diamond(nx, ny) {
                    continue;
                }
                let neighbor = *ctx.grid.cell_native(nx, ny);
                if neighbor.level != level {
                    continue;
                }
                let owner = ctx.scratch.get(nx, ny).region;
                if owner == id {
                    continue;
                }
                if owner != -1 {
                    return false;
                }
                if !(ctx.ids.is_clear(neighbor.tile) || ctx.ids.is_green_lat(neighbor.tile)) {
                    return false;
                }
                ctx.scratch.get_mut(nx, ny).region = id;
                ctx.grid.get_mut(nx, ny).expect("in-diamond cell").level = level;
                members.add(id, [(nx, ny)]);
                next.push((nx, ny));
            }
        }
        frontier = next;
    }
    true
}

/// The boundary collector: region cells with at least one in-diamond
/// neighbor owned by a different region, in native scan order.
fn boundary(ctx: &mut RegionCtx<'_>, members: &Members, id: i32) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for (x, y) in members.in_scan_order(id) {
        if ctx.scratch.get(x, y).region != id {
            continue;
        }
        let is_border = (0..8).any(|dir| {
            let (nx, ny) = RmgGrid::step(x, y, dir);
            ctx.scratch.in_diamond(nx, ny) && ctx.scratch.get(nx, ny).region != id
        });
        if is_border {
            out.push((x, y));
        }
    }
    out
}

/// The water-propagation pass: claims unassigned water-flagged cells
/// reachable from the region's boundary. A no-op on the base path (nothing
/// sets the flag before the mode-3/4 carvers).
fn water_propagate(ctx: &mut RegionCtx<'_>, members: &mut Members, id: i32) {
    let mut queue = boundary(ctx, members, id);
    let mut cursor = 0;
    while cursor < queue.len() {
        let (x, y) = queue[cursor];
        cursor += 1;
        for dir in 0..8 {
            let (nx, ny) = RmgGrid::step(x, y, dir);
            if !ctx.scratch.in_diamond(nx, ny) {
                continue;
            }
            let record = ctx.scratch.get(nx, ny);
            if record.water_region && record.region == -1 {
                ctx.scratch.get_mut(nx, ny).region = id;
                members.add(id, [(nx, ny)]);
                queue.push((nx, ny));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::theater::TheaterCliffRanges;

    fn ids() -> TileIds {
        TileIds {
            clear: 0,
            ramp_base: -1,
            ramp_smooth: -1,
            slope_set_pieces: -1,
            rough: -1,
            sand: -1,
            green: 100,
            rough_lat: -1,
            sand_lat: -1,
            green_lat: 110,
            pave_lat: -1,
            pave: -1,
            water_base: 500,
            shore: 400,
            water_bridge: -1,
            misc_pave: -1,
            paved_roads: -1,
            paved_road_ends: -1,
            medians: -1,
            cliffs: TheaterCliffRanges::default(),
        }
    }

    fn world() -> (RmgGrid, RmgScratch) {
        let (map_w, map_h) = (20, 16);
        let stride = (map_w + map_h + 1) as usize;
        let (dmin, dmax) = (map_w, map_w + 2 * map_h);
        let mut grid = RmgGrid::new(stride, dmin, dmax);
        let scratch = RmgScratch::new(stride, dmin, dmax);
        let coords: Vec<(i32, i32)> = grid.native_cells().collect();
        for (x, y) in coords {
            // Clear land everywhere at the default level.
            grid.get_mut(x, y).unwrap().tile = 0;
        }
        (grid, scratch)
    }

    fn ctx<'a>(
        grid: &'a mut RmgGrid,
        scratch: &'a mut RmgScratch,
        identity: &'a TileIds,
        rng: &'a mut RmgRng,
    ) -> RegionCtx<'a> {
        RegionCtx {
            grid,
            scratch,
            ids: identity,
            rng,
            map_type: 1,
            default_level: 4,
        }
    }

    #[test]
    fn partition_is_deterministic() {
        let outcome = |seed| {
            let (mut grid, mut scratch) = world();
            let identity = ids();
            // A couple of small features to exercise dissolve paths.
            for &(x, y) in &[(14, 9), (15, 9), (16, 10)] {
                if grid.is_valid(x, y) {
                    grid.get_mut(x, y).unwrap().tile = 500;
                }
            }
            let mut rng = RmgRng::new(seed);
            let mut context = ctx(&mut grid, &mut scratch, &identity, &mut rng);
            let regions = run(&mut context);
            let map: Vec<i32> = grid
                .native_cells()
                .collect::<Vec<_>>()
                .iter()
                .map(|&(x, y)| scratch.get(x, y).region)
                .collect();
            (regions.list.len(), map)
        };
        assert_eq!(outcome(42), outcome(42));
    }
}
