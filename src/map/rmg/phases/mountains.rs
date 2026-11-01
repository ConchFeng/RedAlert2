//! VERA's large-map Mountainous layout, before the shared region and cliff passes.
//!
//! Retail Generate598960 only seeds one river system and lake. This extension
//! supplies broad, four-level plateaus on maps beyond the retail 120-cell side;
//! the existing connector, cliff, start, and hill owners finish their terrain.

use super::super::MapGeometry;
use super::super::grid::RmgGrid;
use super::super::rng::RmgRng;
use super::super::tiles::TileIds;
use super::zones::diamond_frame_contains;

const RETAIL_MAX_SIDE: i32 = 120;
const SECTOR_SIDE: i32 = 80;
const CLEAR_APRON: i32 = 10;
const HEIGHT_STEP: u8 = 4;

pub(in crate::map::rmg) fn seed(
    grid: &mut RmgGrid,
    ids: &TileIds,
    rng: &mut RmgRng,
    geometry: &MapGeometry,
    map_type: i32,
) {
    if map_type != 4 || geometry.gen_w.min(geometry.gen_h) <= RETAIL_MAX_SIDE {
        return;
    }
    let columns = (geometry.gen_w / SECTOR_SIDE).max(1);
    let rows = (geometry.gen_h / SECTOR_SIDE).max(1);
    let local_rect = [2, 5, geometry.gen_w, geometry.gen_h];
    for row in 0..rows {
        for column in 0..columns {
            let u = 2 + (2 * column + 1) * geometry.gen_w / (2 * columns);
            let v = 5 + (2 * row + 1) * geometry.gen_h / (2 * rows);
            let center = (
                u + v + 1 + rng.uniform(-6, 6),
                v - u + geometry.map_w + rng.uniform(-6, 6),
            );
            let long = rng.uniform(32, 40);
            let short = rng.uniform(21, 27);
            let (radius_x, radius_y) = if rng.uniform(0, 1) == 0 {
                (long, short)
            } else {
                (short, long)
            };
            let Some(level) = grid.get(center.0, center.1).map(|cell| cell.level) else {
                continue;
            };
            if level > 8 {
                continue;
            }
            let rect = (
                center.0 - radius_x - CLEAR_APRON,
                center.1 - radius_y - CLEAR_APRON,
                2 * (radius_x + CLEAR_APRON) + 1,
                2 * (radius_y + CLEAR_APRON) + 1,
            );
            let clear = (rect.1..rect.1 + rect.3).all(|y| {
                (rect.0..rect.0 + rect.2).all(|x| {
                    grid.get(x, y).is_some_and(|cell| {
                        cell.level == level
                            && ids.is_clear(cell.tile)
                            && diamond_frame_contains(
                                geometry.map_w,
                                local_rect,
                                x,
                                y,
                                level + HEIGHT_STEP,
                                false,
                            )
                    })
                })
            });
            if !clear {
                continue;
            }
            let corner = short / 3;
            for y in center.1 - radius_y..=center.1 + radius_y {
                for x in center.0 - radius_x..=center.0 + radius_x {
                    if (x - center.0).abs() + (y - center.1).abs() <= radius_x + radius_y - corner {
                        grid.get_mut(x, y).expect("admitted plateau").level = level + HEIGHT_STEP;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::rmg::RmgOptions;

    #[test]
    fn plateau_seeding_is_deterministic_and_leaves_retail_maps_unchanged() {
        let geometry = MapGeometry::from_options(&RmgOptions {
            width: 4,
            height: 4,
            num_players: 8,
            ..Default::default()
        });
        let ids = TileIds::from_keys(&Default::default());
        let snapshot = |map_type| {
            let mut grid =
                RmgGrid::new(geometry.stride, geometry.diamond_min, geometry.diamond_max);
            let mut rng = RmgRng::new(36753);
            seed(&mut grid, &ids, &mut rng, &geometry, map_type);
            let levels: Vec<_> = grid
                .native_cells()
                .map(|(x, y)| grid.get(x, y).unwrap().level)
                .collect();
            (levels, rng.next_u32())
        };
        let first = snapshot(4);
        assert!(first.0.contains(&8));
        assert_eq!(first, snapshot(4));
        let untouched = snapshot(3);
        assert!(untouched.0.iter().all(|&level| level == 4));
        assert_eq!(untouched.1, RmgRng::new(36753).next_u32());
        let retail = MapGeometry::from_options(&RmgOptions {
            width: 3,
            height: 3,
            num_players: 8,
            ..Default::default()
        });
        let mut grid = RmgGrid::new(retail.stride, retail.diamond_min, retail.diamond_max);
        let before = grid.clone();
        let mut rng = RmgRng::new(36753);
        seed(&mut grid, &ids, &mut rng, &retail, 4);
        assert!(
            grid.native_cells()
                .all(|(x, y)| grid.get(x, y) == before.get(x, y))
        );
        assert_eq!(rng.next_u32(), RmgRng::new(36753).next_u32());
    }
}
