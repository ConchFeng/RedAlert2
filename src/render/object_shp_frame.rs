//! Shared SHP frame selection for desktop and Horizon presentation.
//! Reads authoritative animation/sequence state without advancing simulation.

use crate::map::entities::EntityCategory;
use crate::sim::animation;

/// The draw already owns a fresh pose. Other callers still use the wrapper
/// above, which reads its own sample and retains same-frame mutation checks.
pub(crate) fn resolve_object_shp_frame_with_facing(
    runtime: Option<&crate::sim::runtime::SimRuntime>,
    rules: Option<&crate::rules::ruleset::RuleSet>,
    type_id: &str,
    entity: &crate::sim::game_entity::GameEntity,
    facing: u16,
    cells: Option<&crate::map::resolved_terrain::NativeCellQuery<'_>>,
) -> Option<u16> {
    let facing = (facing >> 8) as u8;
    // Pass raw facing (not canonical) to resolve_shp_frame so the
    // facing-to-index division works correctly for any facing count
    // (6, 8, 10, etc.). The absolute frame index encodes the direction.
    let type_id = if entity.category() == EntityCategory::Unit {
        runtime.map_or(type_id, |runtime| {
            runtime.simulation.interner().resolve(entity.type_ref())
        })
    } else {
        type_id
    };
    let sequence_set = rules.and_then(|rules| rules.animation_sequence(type_id));
    if entity.category() == EntityCategory::Unit
        && entity.animation.as_ref().is_none_or(|anim_state| {
            matches!(
                anim_state.sequence(),
                animation::SequenceKind::Stand | animation::SequenceKind::Walk
            )
        })
        && let Some(set) = sequence_set
    {
        // A retained signed/wide native layout can be outside the atlas frame
        // representation. Do not replace that failure with an infantry-facing
        // fallback, which would draw an unrelated valid frame.
        return animation::resolve_shp_vehicle_body_frame(
            set,
            facing,
            entity.body_frame_counter.get(),
            entity.unit_firing_counter.get(),
            // The draw asks the locomotor's Is_Moving (vt+0x10 at `0x0073C696`).
            crate::sim::movement::motion_query::is_moving(entity) == Some(true),
            entity
                .disguise
                .as_ref()
                .is_some_and(|disguise| disguise.is_disguised()),
        );
    }
    if let (Some((doing, stage)), Some(set)) = (entity.infantry_sprite_pose(), sequence_set) {
        // Original518D93..518DC8: Doing-1 draws Tread16 only when the
        // current CellClass+EC is Water2 and OnBridge is clear. This reads
        // live map land; it does not depend on the separate Infantry6E8 byte.
        let selected = if doing == -1 {
            let water = cells.is_some_and(|cells| {
                let cell = cells.lookup((entity.position.rx() as i16, entity.position.ry() as i16));
                cells.land_type(cell) == 2
            });
            if water && !entity.on_bridge() { 16 } else { 0 }
        } else {
            doing
        };
        let record = set.infantry_action(selected)?;
        let facing = rules
            .and_then(|rules| {
                let sim = &runtime?.simulation;
                sim.interner().object_type(entity.type_ref(), rules)
            })
            .filter(|object| object.jumpjet && !object.jumpjet_turn)
            .filter(|_| {
                entity.locomotor.as_ref().is_some_and(|loco| {
                    loco.active_kind() == crate::rules::locomotor_type::LocomotorKind::Jumpjet
                })
            })
            .and(entity.attack_target.target())
            .and_then(|target| {
                let sim = &runtime?.simulation;
                crate::sim::movement::turret::facing_toward_target(entity, &target, sim)
            })
            .map_or(facing, |direction| (direction >> 8) as u8);
        // Native EAX remains signed. A negative/wide result cannot alias an
        // unrelated valid frame in the u16 SHP atlas. This is an asset boundary,
        // not evidence for native invalid-index shape-access behavior.
        return u16::try_from(animation::resolve_shp_frame(record, facing, stage)).ok();
    }
    if let (Some(anim_state), Some(set)) = (entity.animation.as_ref(), sequence_set)
        && let Some(def) = set.get(&anim_state.sequence())
    {
        return u16::try_from(animation::resolve_shp_frame(
            def,
            facing,
            i32::from(anim_state.frame_index()),
        ))
        .ok();
    }
    // Fallback when no sequence data was built for this type: the standing
    // block is frames 0..7, so the facing slot is the frame index. Uses the
    // same native facing table as the real path so the two cannot disagree.
    Some(animation::infantry_facing_slot(facing))
}
