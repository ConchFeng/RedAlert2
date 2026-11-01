//! EBolt4C2830 reverse tactical traversal and4C29E0 scene cleanup. These
//! copied effects have no Abstract ID or save record. Only a Unit's FireAt
//! bolt links to its source ([`ElectricBolts::follow_owners`]). Unlike lasers
//! they age on each tactical composite, even without Logic.

use crate::render::electric_bolt::{ElectricBoltDraw, ElectricBoltPalette};
use crate::render::surface_line::{SurfaceDraw, SurfaceLine, SurfaceLineViewport};
use crate::sim::combat::electric_bolt::{ElectricBoltBirth, ElectricBoltOwner};
use crate::sim::projectile::ProjectileCoord;
use crate::sim::rng::{MainRng, MainRngDraws};

struct Bolt {
    birth: ElectricBoltBirth,
    phase: i32,
    decay: i32,
}

struct BoltProjection {
    draw: ElectricBoltDraw,
    palette: ElectricBoltPalette,
    world_lines: Vec<SurfaceLine>,
    admitted: bool,
    seed: u64,
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct ElectricBoltObservation {
    birth_frame: i32,
    from: [i32; 3],
    to: [i32; 3],
    z_adjust: i32,
    alternate_color: bool,
    phase: i32,
    decay: i32,
}

#[derive(Default)]
pub(crate) struct ElectricBolts {
    live: Vec<Bolt>,
    lines: Vec<SurfaceLine>,
    projections: Vec<BoltProjection>,
}

impl ElectricBolts {
    pub(crate) fn create(&mut self, mut birth: ElectricBoltBirth) {
        // SpawnElectricBoltEffect (`0x006FD5F2..0x006FD606`) links a bolt
        // only into an empty Unit `+0x6DC`; the slot holds the Unit's live
        // owned bolt until that bolt's last pass (`0x004C29A9`).
        if let Some(owner) = birth.owner
            && self
                .live
                .iter()
                .any(|bolt| bolt.birth.owner.is_some_and(|held| held.unit == owner.unit))
        {
            birth.owner = None;
        }
        self.live.push(Bolt {
            phase: birth.phase,
            birth,
            decay: 0x10000,
        });
    }

    /// Manager4C2830's owner arm, ahead of each bolt's draw: an owned bolt's
    /// source becomes its owner's current FLH, unless that answers the static
    /// zero coordinate (`0x008A0E50`). `resolve` answers `None` once the Unit
    /// is gone; its destructor's ClearOwner (`0x00735789`) leaves the bolt at
    /// its last source.
    pub(crate) fn follow_owners(
        &mut self,
        mut resolve: impl FnMut(ElectricBoltOwner) -> Option<ProjectileCoord>,
    ) {
        for bolt in self.live.iter_mut().rev() {
            let Some(owner) = bolt.birth.owner else {
                continue;
            };
            match resolve(owner) {
                Some(source) if source != ProjectileCoord::new(0, 0, 0) => {
                    bolt.birth.from = source;
                }
                Some(_) => {}
                None => bolt.birth.owner = None,
            }
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.live.is_empty()
    }

    /// Manager4C2830 never uses elapsed time. A rejected clip still increments
    /// phase and shifts lifetime; a zero entry skips projection and increment.
    /// Geometry alone borrows Main. Upload/chunk replay only reads its output.
    pub(crate) fn composite(
        &mut self,
        viewport: SurfaceLineViewport,
        palette: Option<ElectricBoltPalette>,
        main: &mut MainRngDraws,
        mut visible: impl FnMut(&ElectricBoltBirth) -> bool,
    ) -> &[SurfaceLine] {
        self.lines.clear();
        self.projections.clear();
        for bolt in self.live.iter_mut().rev() {
            if bolt.decay != 0 {
                let draw = ElectricBoltDraw {
                    from: bolt.birth.from,
                    to: bolt.birth.to,
                    z_adjust: bolt.birth.z_adjust,
                    phase: bolt.phase,
                    alternate_color: bolt.birth.alternate_color,
                };
                // EBoltMgr::UpdateAndDrawAll @ 0x004C2830 calls Draw4C1F20
                // before incrementing phase at4C293D, then shifts decay at
                //4C2942. Conceal only emitted pixels: the optional fog style
                // must not skip geometry's shared Main draws or either update.
                let emit = visible(&bolt.birth);
                let palette = palette.expect("loaded tactical PALETTE.PAL");
                let first_line = self.lines.len();
                let admitted = draw.lines(viewport, palette, main, |line| {
                    if emit {
                        self.lines.push(line);
                    }
                });
                if emit {
                    let world_lines = self.lines[first_line..]
                        .iter()
                        .copied()
                        .map(|line| translate_line(line, viewport.camera))
                        .collect();
                    self.projections.push(BoltProjection {
                        draw,
                        palette,
                        world_lines,
                        admitted,
                        seed: projection_seed(bolt.birth, bolt.phase),
                    });
                }
                bolt.phase = bolt.phase.wrapping_add(1);
            }
            bolt.decay >>= 1;
        }
        self.live.retain(|bolt| bolt.decay != 0);
        &self.lines
    }

    /// Additional views reuse admitted world geometry without another Main draw
    /// or manager visit. A main-clipped bolt has no retained geometry; its
    /// auxiliary-only shape uses a birth/phase-local stream, never process Main.
    pub(crate) fn project(&self, viewport: SurfaceLineViewport) -> Vec<SurfaceDraw> {
        let mut lines = Vec::new();
        for projection in &self.projections {
            if projection.admitted {
                lines.extend(projection.world_lines.iter().copied().map(|line| {
                    SurfaceDraw::Line(translate_line(line, viewport.camera.map(i32::wrapping_neg)))
                }));
            } else {
                let local = MainRng::new(projection.seed);
                projection
                    .draw
                    .lines(viewport, projection.palette, &mut local.draws(), |line| {
                        lines.push(SurfaceDraw::Line(line));
                    });
            }
        }
        lines
    }

    pub(crate) fn observations(&self) -> impl Iterator<Item = ElectricBoltObservation> + '_ {
        self.live.iter().map(|bolt| ElectricBoltObservation {
            birth_frame: bolt.birth.frame,
            from: [bolt.birth.from.x, bolt.birth.from.y, bolt.birth.from.z],
            to: [bolt.birth.to.x, bolt.birth.to.y, bolt.birth.to.z],
            z_adjust: bolt.birth.z_adjust,
            alternate_color: bolt.birth.alternate_color,
            phase: bolt.phase,
            decay: bolt.decay,
        })
    }

    /// Scenario53493F clears the entire nonserialized vector. Source death
    /// alone does not detach an ordinary6FD460 bolt (its owner link is null).
    pub(crate) fn clear_on_load(&mut self) {
        self.live.clear();
        self.lines.clear();
        self.projections.clear();
    }
}

fn translate_line(mut line: SurfaceLine, offset: [i32; 2]) -> SurfaceLine {
    for point in [&mut line.from, &mut line.to] {
        for axis in 0..2 {
            point[axis] = point[axis].wrapping_add(offset[axis]);
        }
    }
    line
}

fn projection_seed(birth: ElectricBoltBirth, phase: i32) -> u64 {
    [
        birth.frame,
        birth.from.x,
        birth.from.y,
        birth.from.z,
        birth.to.x,
        birth.to.y,
        birth.to.z,
        birth.z_adjust,
        phase,
        i32::from(birth.alternate_color),
    ]
    .into_iter()
    .fold(0x4542_4f4c_u64, |seed, value| {
        seed.rotate_left(7) ^ u64::from(value as u32)
    })
}

#[cfg(test)]
#[path = "electric_bolt_lifetime_tests.rs"]
mod tests;
