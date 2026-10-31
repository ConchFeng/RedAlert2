//! VERA-only: the facts a path batch's Can_Enter_Cell asks share, copied once
//! per batch. A batch's searches ([`crate::sim::movement::path_batch`]) all
//! read one unchanged world, and in a jam each real cell is asked about ~36
//! times a batch and each blocker read ~42 times, every time from scattered
//! terrain, occupancy and entity records. The first ask copies what Infantry
//! `0x0051BF90` / Unit `0x0073F0A0` read of the cell or blocker into a dense
//! table and later asks read the copy. The world stays the source of truth:
//! the tables are emptied before each batch, never saved or hashed, and debug
//! builds compare every reuse with a fresh read. Asks outside a batch read
//! the world directly.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::map::cell_index::NativeCellIdentity as Cell;
use crate::map::entities::EntityCategory;
use crate::rules::locomotor_type::SpeedType;
use crate::rules::terrain_rules::SpeedCostProfile;
use crate::sim::components::DriveCoord;
use crate::sim::game_entity::GameEntity;
use crate::sim::intern::InternedId;
use crate::sim::movement::bump_crush::CrushTarget;
use crate::sim::movement::locomotor::MovementLayer;
use crate::sim::occupancy::RawCellKey;
use crate::sim::overlay_grid::OverlayCell;
use crate::sim::world::Simulation;

const CELLS_PER_BLOCK: usize = u64::BITS as usize;

struct CellFactsBlock {
    cells: [OnceLock<CellFacts>; CELLS_PER_BLOCK],
    plain: [AtomicU32; CELLS_PER_BLOCK],
    used: AtomicU64,
}

impl Default for CellFactsBlock {
    fn default() -> Self {
        Self {
            cells: std::array::from_fn(|_| OnceLock::new()),
            plain: std::array::from_fn(|_| AtomicU32::default()),
            used: AtomicU64::new(0),
        }
    }
}

impl CellFactsBlock {
    fn mark_used(&self, index: usize) {
        let bit = 1u64 << index;
        if self.used.load(Ordering::Relaxed) & bit == 0 {
            self.used.fetch_or(bit, Ordering::Relaxed);
        }
    }

    fn reset(&mut self) {
        // Joined batch readers only set bits; this exclusive reset clears them.
        let mut used = std::mem::take(self.used.get_mut());
        if used == u64::MAX {
            for cell in &mut self.cells {
                cell.take();
            }
            for plain in &mut self.plain {
                *plain.get_mut() = 0;
            }
            return;
        }
        while used != 0 {
            let index = used.trailing_zeros() as usize;
            self.cells[index].take();
            *self.plain[index].get_mut() = 0;
            used &= used - 1;
        }
    }
}

/// One batch's [`CellFacts`] and [`PlainCell`] by real cell index and
/// [`BlockerFacts`] by entity slot, each kept from its first ask.
#[derive(Default)]
pub(crate) struct EntryFacts {
    cells: Vec<CellFactsBlock>,
    cell_count: usize,
    blockers: Vec<OnceLock<BlockerFacts>>,
}

impl std::fmt::Debug for EntryFacts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntryFacts")
            .field("cells", &self.cell_count)
            .field("blockers", &self.blockers.len())
            .finish()
    }
}

impl EntryFacts {
    /// Forget every kept fact and fit the tables to `cells` real cells and
    /// `slots` entity slots, before a batch.
    pub(crate) fn reset(&mut self, cells: usize, slots: usize) {
        crate::sim::parallel::for_each_mut(&mut self.cells, CellFactsBlock::reset);
        self.cells
            .resize_with(cells.div_ceil(CELLS_PER_BLOCK), CellFactsBlock::default);
        self.cell_count = cells;
        crate::sim::parallel::for_each_mut(&mut self.blockers, |kept| {
            kept.take();
        });
        self.blockers.resize_with(slots, OnceLock::new);
    }

    /// Real cell `index`'s facts, kept from this batch's first ask.
    pub(super) fn cell(&self, index: usize, read: impl Fn() -> CellFacts) -> &CellFacts {
        let (block, offset) = self.cell_slot(index);
        let kept = block.cells[offset].get_or_init(|| {
            let value = read();
            block.mark_used(offset);
            value
        });
        debug_assert!(
            !crate::sim::touch_log::live_read_check_enabled() || *kept == read(),
            "real cell {index} changed during a path batch"
        );
        kept
    }

    /// Real cell `index`'s [`PlainCell`], kept from this batch's first ask.
    /// Searches asking at once may each read it and store the same bits:
    /// every read sees the one unchanged world.
    pub(super) fn plain(&self, index: usize, read: impl Fn() -> PlainCell) -> PlainCell {
        let (block, offset) = self.cell_slot(index);
        let kept = &block.plain[offset];
        let cell = match kept.load(Ordering::Relaxed) {
            0 => {
                let cell = read();
                block.mark_used(offset);
                kept.store(cell.0, Ordering::Relaxed);
                cell
            }
            bits => PlainCell(bits),
        };
        debug_assert!(
            !crate::sim::touch_log::live_read_check_enabled() || cell == read(),
            "real cell {index} changed during a path batch"
        );
        cell
    }

    fn cell_slot(&self, index: usize) -> (&CellFactsBlock, usize) {
        assert!(index < self.cell_count, "cell facts index out of bounds");
        (
            &self.cells[index / CELLS_PER_BLOCK],
            index % CELLS_PER_BLOCK,
        )
    }

    /// The facts of the entity in `slot`, kept from this batch's first ask.
    pub(super) fn blocker(&self, slot: usize, read: impl Fn() -> BlockerFacts) -> &BlockerFacts {
        let kept = self.blockers[slot].get_or_init(&read);
        debug_assert!(
            !crate::sim::touch_log::live_read_check_enabled() || *kept == read(),
            "the entity in slot {slot} changed during a path batch"
        );
        kept
    }
}

/// What Can_Enter_Cell reads of one real cell. Its object lists' members, its
/// tube, the first-Building lookups and MovementRestrictedTo's land, tile and
/// overlay reads stay live; a [`PlainCell`] keeps only whether it has a tube.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CellFacts {
    pub(super) coord: (i16, i16),
    pub(super) level: i8,
    pub(super) slope: u8,
    pub(super) flags: u32,
    /// The raw ground and deck occupation bytes, each with its infantry owner.
    pub(super) ground: (u8, Option<InternedId>),
    pub(super) deck: (u8, Option<InternedId>),
    pub(super) in_playfield: bool,
    pub(super) overlay: OverlayCell,
    pub(super) terrain_object: bool,
    /// Whether the ground and deck object lists yield anything, a terrain
    /// object included.
    pub(super) ground_listed: bool,
    pub(super) deck_listed: bool,
    pub(super) speed_costs: SpeedCostProfile,
}

impl CellFacts {
    /// Real cell `index` as the world holds it now. Reads no lookup, so it
    /// never stamps the shared Dummy.
    pub(super) fn of(sim: &Simulation, index: usize) -> Self {
        let terrain = sim
            .resolved_terrain
            .as_ref()
            .expect("live cell-entry terrain");
        let cell = Cell::Real(index);
        let c = &terrain.cells()[index];
        let key = RawCellKey::from_native(terrain, cell);
        let raw = &sim.substrate.raw_cell_occupation;
        let terrain_object = sim.terrain_objects.cell_index().get(&(c.rx, c.ry)).copied();
        let listed = |layer| {
            sim.substrate
                .occupancy
                .cell_objects(c.rx, c.ry, layer, terrain_object)
                .next()
                .is_some()
        };
        Self {
            coord: terrain.native_cell_coord(cell),
            level: c.level as i8,
            slope: c.slope_type,
            flags: terrain.native_cell_flags(cell),
            ground: raw.occupant_at(key, MovementLayer::Ground),
            deck: raw.occupant_at(key, MovementLayer::Bridge),
            in_playfield: crate::sim::cell_rect::retained_cell_is_in_playfield(
                cell,
                sim.playfield_bounds,
                &crate::map::resolved_terrain::NativeCellQuery::canonical(terrain),
            ),
            overlay: sim.cell_overlay(cell),
            terrain_object: terrain_object.is_some(),
            ground_listed: listed(MovementLayer::Ground),
            deck_listed: listed(MovementLayer::Bridge),
            speed_costs: c.speed_costs,
        }
    }

    /// Whether `layer`'s object list yields anything. Other layers' lists are
    /// walked live.
    pub(super) fn listed(&self, layer: MovementLayer) -> bool {
        match layer {
            MovementLayer::Ground => self.ground_listed,
            MovementLayer::Bridge => self.deck_listed,
            MovementLayer::Air | MovementLayer::Underground => true,
        }
    }
}

/// What a step from a flat cell onto an empty one reads of each cell
/// ([`super::plain_entry`]), packed in one word: a search floods mostly open
/// ground, and these asks then read four bytes per cell instead of its
/// [`CellFacts`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PlainCell(u32);

impl PlainCell {
    /// Set in every kept word, so 0 is "not asked yet".
    const KNOWN: u32 = 1 << 31;
    /// No structural bridge (`0x100`) and no tube: a step into or out of the
    /// cell reads nothing else of its bridge or tube.
    const FLAT: u32 = 1 << 30;
    /// Also no raw ground occupation, nothing in the ground object list (a
    /// terrain object included), and inside the playfield.
    const EMPTY: u32 = 1 << 29;
    /// An overlay, whose type's flags the asker reads.
    const OVERLAY: u32 = 1 << 28;
    /// The SpeedTypes this word answers for, in discriminant order.
    const SPEEDS: [SpeedType; 8] = [
        SpeedType::Foot,
        SpeedType::Track,
        SpeedType::Wheel,
        SpeedType::Hover,
        SpeedType::Winged,
        SpeedType::Float,
        SpeedType::Amphibious,
        SpeedType::FloatBeach,
    ];

    /// The bits of the cell with `facts`, holding a tube when `tube`.
    pub(super) fn new(facts: &CellFacts, tube: bool) -> Self {
        let flat = facts.flags & 0x100 == 0 && !tube;
        let empty = flat && facts.ground == (0, None) && !facts.ground_listed && facts.in_playfield;
        // Bits 8..16: the land row resolved for SpeedType n; 16..24: nonzero.
        let rows = Self::SPEEDS
            .iter()
            .enumerate()
            .map(|(n, &speed)| {
                facts
                    .speed_costs
                    .cost_for_speed_type(speed)
                    .map_or(0, |cost| {
                        (1 << (8 + n)) | (u32::from(cost != 0) << (16 + n))
                    })
            })
            .fold(0, |rows, row| rows | row);
        Self(
            Self::KNOWN
                | if flat { Self::FLAT } else { 0 }
                | if empty { Self::EMPTY } else { 0 }
                | if facts.overlay.overlay_id.is_some() {
                    Self::OVERLAY
                } else {
                    0
                }
                | rows
                | u32::from(facts.level as u8),
        )
    }

    pub(super) fn flat(self) -> bool {
        self.0 & Self::FLAT != 0
    }

    pub(super) fn empty(self) -> bool {
        self.0 & Self::EMPTY != 0
    }

    pub(super) fn overlay(self) -> bool {
        self.0 & Self::OVERLAY != 0
    }

    pub(super) fn level(self) -> i8 {
        self.0 as u8 as i8
    }

    /// Whether the land row for `speed` is nonzero, when the cell has one
    /// ([`CellFacts::speed_costs`]).
    pub(super) fn row_nonzero(self, speed: SpeedType) -> Option<bool> {
        let n = Self::SPEEDS.iter().position(|&known| known == speed)?;
        (self.0 >> (8 + n) & 1 != 0).then_some(self.0 >> (16 + n) & 1 != 0)
    }
}

/// What Can_Enter_Cell reads of one blocker in a cell's object list, at one
/// frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct BlockerFacts {
    pub(super) owner: InternedId,
    pub(super) type_ref: InternedId,
    /// Its crush inputs, with its category and Iron Curtain.
    pub(super) crush: CrushTarget,
    /// Its world coordinate, its cell's x and its body facing.
    pub(super) position: DriveCoord,
    pub(super) rx: u16,
    pub(super) facing: u16,
    /// `motion_query::is_moving`, and whether it has a NavCom or turns.
    pub(super) moving: Option<bool>,
    pub(super) nav_com: bool,
    pub(super) rotating: bool,
    pub(super) warped_out: bool,
    pub(super) fully_cloaked: bool,
    pub(super) disguised: bool,
    pub(super) open_gate: bool,
    pub(super) bunker_occupied: bool,
    pub(super) foot_occupation: bool,
    pub(super) occupant_slot_a4: bool,
}

impl BlockerFacts {
    /// VERA-only: a per-batch read-only copy of each blocker's category,
    /// owner, type, cloak, crush target, motion, facing and position, with
    /// its warp, disguise, gate, bunker, occupation and track-slot bits, for
    /// classify_foot_entry (Infantry `0x0051BF90` / Unit `0x0073F0A0`); the
    /// entity records are the source of truth, rebuilt every batch. An ask
    /// outside a batch reads it fresh.
    pub(super) fn of(b: &GameEntity, frame: u32) -> Self {
        Self {
            owner: b.owner(),
            type_ref: b.type_ref(),
            crush: CrushTarget::from_entity(b, frame),
            position: crate::sim::movement::ground_pose::position_world_coord(&b.position),
            rx: b.position.rx(),
            facing: b.body_facing_current(frame),
            moving: crate::sim::movement::motion_query::is_moving(b),
            nav_com: b.navigation.nav_com().is_some(),
            rotating: b.body_facing.is_rotating(frame),
            warped_out: b.is_warped_out(),
            fully_cloaked: b.cloak.as_ref().is_some_and(|s| s.is_fully_cloaked()),
            disguised: b.disguise.as_ref().is_some_and(|s| s.is_disguised()),
            open_gate: b.is_open_gate(),
            bunker_occupied: b.bunker.occupant().is_some(),
            foot_occupation: b.foot_occupation_enabled(),
            occupant_slot_a4: crate::sim::movement::drive_track::occupant_slot_a4_answers_true(b),
        }
    }

    pub(super) fn category(&self) -> EntityCategory {
        self.crush.category
    }

    /// Iron Curtain or Force Shield at that frame.
    pub(super) fn invulnerable(&self) -> bool {
        self.crush.iron_curtained
    }
}

#[cfg(any(test, feature = "test-support"))]
impl EntryFacts {
    /// An empty table fitted to `sim`, as a batch starts.
    pub(crate) fn fitted(sim: &Simulation) -> Self {
        let mut facts = Self::default();
        facts.reset(
            sim.resolved_terrain
                .as_ref()
                .map_or(0, |terrain| terrain.cells().len()),
            sim.substrate.entities.slot_count(),
        );
        facts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_first_fills_keep_exact_bounds_across_shrink_and_regrow() {
        let (sim, _, _) = crate::sim::world::entry_test_fixture::fixture();
        let mut value = CellFacts::of(&sim, 511);
        let mut facts = EntryFacts::default();
        facts.reset(65, 0);
        std::thread::scope(|scope| {
            for _ in 0..4 {
                let facts = &facts;
                scope.spawn(move || {
                    for _ in 0..32 {
                        for index in [63, 64] {
                            assert_eq!(*facts.cell(index, || value), value);
                            assert_eq!(
                                facts.plain(index, || PlainCell::new(&value, false)),
                                PlainCell::new(&value, false),
                            );
                        }
                    }
                });
            }
        });
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                facts.cell(65, || value);
            }))
            .is_err()
        );
        value.level = 3;
        facts.reset(64, 0);
        assert_eq!(*facts.cell(63, || value), value);
        assert_eq!(facts.plain(63, || PlainCell::new(&value, false)).level(), 3);
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                facts.cell(64, || value);
            }))
            .is_err()
        );
        value.level = 6;
        facts.reset(65, 0);
        assert_eq!(*facts.cell(64, || value), value);
        assert_eq!(facts.plain(64, || PlainCell::new(&value, false)).level(), 6);
    }

    #[test]
    #[ignore = "manual cache performance probe; run release alone with --nocapture"]
    fn sparse_and_dense_first_use_and_reset_cost() {
        use std::hint::black_box;
        use std::time::Instant;

        const CELLS: usize = 1024 * 1024;
        const MEASURED: u32 = 12;
        let (sim, _, _) = crate::sim::world::entry_test_fixture::fixture();
        let value = black_box(CellFacts::of(&sim, 511));
        for (name, indices) in [
            (
                "sparse",
                (0..4096)
                    .map(|i| (i / 64) * 1024 + i % 64)
                    .collect::<Vec<_>>(),
            ),
            ("dense", (0..CELLS).collect()),
        ] {
            let mut facts = EntryFacts::default();
            facts.reset(CELLS, 0);
            let mut cycle = || {
                let cached = black_box(&facts);
                crate::sim::parallel::map(&indices, |&index| {
                    let cell = cached.cell(index, || value);
                    black_box(cached.plain(index, || PlainCell::new(cell, false)));
                });
                facts.reset(CELLS, 0);
            };
            for _ in 0..3 {
                cycle();
            }
            let started = Instant::now();
            for _ in 0..MEASURED {
                cycle();
            }
            println!(
                "entry_facts_cycle case={name} cells={CELLS} queried={} mean_ms={:.6}",
                indices.len(),
                started.elapsed().as_secs_f64() * 1000.0 / f64::from(MEASURED),
            );
        }
    }
}
