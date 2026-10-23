//! Retail in-game sidebar for the Horizon live match.
//!
//! The layout model, SHP atlas loaders and GAME.FNT stay shared with the
//! desktop client through the `ui::sidebar` and `render::*` path modules.

use anyhow::{Context, Result};
use std::collections::HashMap;

use crate::assets::asset_manager::AssetManager;
use crate::assets::fnt_file::FntFile;
use crate::assets::pal_file::Palette;
use crate::gpu::GpuContext;
use crate::map::theater::TheaterData;
use crate::render::batch::{BatchRenderer, InstanceBufferPool, SpriteInstance};
use crate::render::bit_font::BitFont;
use crate::render::sidebar_cameo_atlas::{self, SidebarCameoAtlas};
use crate::render::sidebar_chrome::{self, SidebarChromeAtlas, SidebarChromeSet, SidebarTheme};
use crate::render::sidebar_text;
use crate::rules::ruleset::RuleSet;
use crate::sidebar_eva::{CameoClick, CameoOrder, CameoPress, cameo_click};
use crate::sim::intern::InternedId;
use crate::sim::production::{
    ProductionCategory, active_producer_for_owner_category, all_build_options_for_owner,
    cameo_draw_state, category_for_object, credits_for_owner, initial_construction_options,
    power_balance_for_owner, queue_view_for_owner, ready_buildings_for_owner,
};
use crate::sim::superweapon::superweapon_views_for_owner;
use crate::sim::world::{LifecycleOutput, Simulation};
use crate::ui_sidebar as sidebar;
use crate::ui_sidebar::cameo_order::{Cameo, CameoId, CameoKey, CameoStrips};
use crate::ui_sidebar::gadget_flash::SidebarGadgetState;

/// One consumed sidebar click, expressed in the same command vocabulary as the
/// desktop client. `ArmBuildingPlacement` waits for the next tactical click.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarOutcome {
    NotInSidebar,
    Consumed,
    Command(crate::sim::command::Command),
    ArmBuildingPlacement(InternedId),
}

pub struct SwitchSidebar {
    chrome: SidebarChromeSet,
    cameos: Option<SidebarCameoAtlas>,
    font: BitFont,
    view: sidebar::SidebarView,
    strips: CameoStrips,
    active_tab: sidebar::SidebarTab,
    theme: SidebarTheme,
    gadgets: SidebarGadgetState,
    pending_additions: Vec<InternedId>,
    pending_recalculate: bool,
    armed_building: Option<InternedId>,
    factory_links: HashMap<InternedId, CameoFactoryLink>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CameoFactoryLink {
    owner: InternedId,
    category: ProductionCategory,
    construction_order: u64,
}

impl SwitchSidebar {
    pub fn load(
        gpu: &GpuContext,
        batch: &BatchRenderer,
        assets: &mut AssetManager,
        theater: &TheaterData,
        sim: &Simulation,
        rules: &RuleSet,
    ) -> Result<Self> {
        let owner = sim
            .session()
            .current_house()
            .context("sidebar has no local House")?;
        let side = sim.session().scenario_side();
        for cameo_mix in ["cameomd.mix", "cameo.mix"] {
            match assets.load_nested(cameo_mix) {
                Ok(()) => log::info!("Loaded nested {cameo_mix} for Switch sidebar cameos"),
                Err(_) => log::debug!("{cameo_mix} not found"),
            }
        }
        let chrome = sidebar_chrome::build_sidebar_chrome_set(gpu, batch, assets, rules, side)?
            .context("retail sidebar chrome unavailable")?;
        let theme = sidebar_chrome::sidebar_theme_for_side(side);
        let palette = ["cameo.pal", "cameomd.pal", "mousepal.pal", "anim.pal"]
            .into_iter()
            .find_map(|name| {
                assets
                    .get_ref(name)
                    .and_then(|bytes| Palette::from_bytes(bytes).ok())
            })
            .unwrap_or_else(|| {
                Palette::from_rgb(|index| {
                    let c = theater.unit_palette.color(index);
                    [c.r, c.g, c.b]
                })
            });
        let cameos =
            sidebar_cameo_atlas::build_sidebar_cameo_atlas(gpu, batch, assets, rules, &palette);
        let fnt = assets
            .get_ref("GAME.FNT")
            .and_then(|data| FntFile::from_bytes(data).ok())
            .context("retail GAME.FNT unavailable")?;
        let font = BitFont::from_fnt(&gpu.device, &gpu.queue, batch, &fnt);
        let mut strips = CameoStrips::default();
        let reconciled = strips.reconcile(
            &candidates(sim, rules, owner),
            VISIBLE_ROWS,
            true,
            &initial_construction_options(sim, rules),
        );
        let mut gadgets = SidebarGadgetState::new();
        for tab in sidebar::SidebarTab::all() {
            gadgets.set_tab_disabled(tab.tab_index(), false);
        }
        let mut active_tab = sidebar::SidebarTab::Building;
        apply_transitions(
            reconciled.transitions(),
            &mut active_tab,
            &mut gadgets,
            &strips,
        );
        let factory_links = seed_factory_links(sim, owner, &strips);
        let atlas = chrome
            .for_theme(theme)
            .context("sidebar theme unavailable")?;
        let view = build_view(
            sim,
            rules,
            owner,
            &strips,
            active_tab,
            atlas,
            theme,
            &gadgets,
            None,
            &factory_links,
        )?;
        Ok(Self {
            chrome,
            cameos,
            font,
            view,
            strips,
            active_tab,
            theme,
            gadgets,
            pending_additions: Vec::new(),
            pending_recalculate: false,
            armed_building: None,
            factory_links,
        })
    }

    pub fn update(
        &mut self,
        sim: &Simulation,
        rules: &RuleSet,
        lifecycle: &[LifecycleOutput],
    ) -> Result<()> {
        let owner = sim
            .session()
            .current_house()
            .context("sidebar lost local House")?;
        for output in lifecycle {
            match *output {
                LifecycleOutput::SidebarConstructionOption {
                    owner: reached,
                    type_id,
                } if reached == owner => self.pending_additions.push(type_id),
                LifecycleOutput::SidebarRecalculated { owner: reached } if reached == owner => {
                    self.pending_recalculate = true;
                }
                LifecycleOutput::ProductionFactoryLinked {
                    owner: reached,
                    type_id,
                    category,
                    construction_order,
                } if reached == owner => {
                    self.factory_links.insert(
                        type_id,
                        CameoFactoryLink {
                            owner: reached,
                            category,
                            construction_order,
                        },
                    );
                }
                LifecycleOutput::ProductionFactoryAbandoning {
                    owner: reached,
                    category,
                    construction_order,
                } if reached == owner
                    && !self.factory_links.values().any(|link| {
                        link.owner == reached
                            && link.category == category
                            && link.construction_order == construction_order
                    }) =>
                {
                    // The unlinked predecessor can still own the cursor when
                    // House abandonment precedes a successor Begin.
                    if matches!(
                        category,
                        ProductionCategory::Building | ProductionCategory::Defense
                    ) {
                        self.armed_building = None;
                    }
                }
                LifecycleOutput::ProductionFactoryUnlinked {
                    owner: reached,
                    category,
                    construction_order,
                } if reached == owner => {
                    let target = CameoFactoryLink {
                        owner: reached,
                        category,
                        construction_order,
                    };
                    let linked: Vec<_> = self
                        .factory_links
                        .iter()
                        .filter_map(|(type_id, link)| (*link == target).then_some(*type_id))
                        .collect();
                    for type_id in linked {
                        self.strips.clear_request(CameoId::Object(type_id));
                        self.factory_links.remove(&type_id);
                        if self.armed_building == Some(type_id) {
                            self.armed_building = None;
                        }
                    }
                }
                LifecycleOutput::BuildingPlacementSucceeded { owner: reached }
                    if reached == owner =>
                {
                    self.armed_building = None;
                }
                _ => {}
            }
        }
        let candidates = candidates(sim, rules, owner);
        let recalculate = self.pending_recalculate;
        let reconciled = self.strips.reconcile(
            &candidates,
            VISIBLE_ROWS,
            recalculate,
            &self.pending_additions,
        );
        apply_transitions(
            reconciled.transitions(),
            &mut self.active_tab,
            &mut self.gadgets,
            &self.strips,
        );
        self.pending_additions.clear();
        self.pending_recalculate = false;
        let atlas = self
            .chrome
            .for_theme(self.theme)
            .context("sidebar theme disappeared")?;
        let view = build_view(
            sim,
            rules,
            owner,
            &self.strips,
            self.active_tab,
            atlas,
            self.theme,
            &self.gadgets,
            self.armed_building,
            &self.factory_links,
        )?;
        self.view = view;
        Ok(())
    }

    /// Route a click through the retained sidebar surface. The returned
    /// command still goes through the match's ordinary local-command ingress.
    pub fn click(
        &mut self,
        sim: &Simulation,
        rules: &RuleSet,
        point: [f32; 2],
        right: bool,
        shift: bool,
    ) -> SidebarOutcome {
        if !self.view.panel_rect.contains(point[0], point[1]) {
            return SidebarOutcome::NotInSidebar;
        }
        // Tabs precede the cameo grid; disabled strips cannot become active.
        for tab in &self.view.tabs {
            if tab.rect.contains(point[0], point[1]) {
                if tab.disabled || tab.tab == self.active_tab {
                    return SidebarOutcome::Consumed;
                }
                self.active_tab = tab.tab;
                self.rebuild_view(sim, rules);
                return SidebarOutcome::Consumed;
            }
        }
        if self.view.repair_button.rect.contains(point[0], point[1]) {
            if !self.view.repair_button.disabled {
                self.gadgets.repair_mode_on = !self.gadgets.repair_mode_on;
                if self.gadgets.repair_mode_on {
                    self.gadgets.sell_mode_on = false;
                    self.armed_building = None;
                }
                self.rebuild_view(sim, rules);
            }
            return SidebarOutcome::Consumed;
        }
        if self.view.sell_button.rect.contains(point[0], point[1]) {
            if !self.view.sell_button.disabled {
                self.gadgets.sell_mode_on = !self.gadgets.sell_mode_on;
                if self.gadgets.sell_mode_on {
                    self.gadgets.repair_mode_on = false;
                    self.armed_building = None;
                }
                self.rebuild_view(sim, rules);
            }
            return SidebarOutcome::Consumed;
        }
        let page = self.view.layout.side2_tile_count.max(1);
        if self
            .view
            .scroll_down_button
            .rect
            .contains(point[0], point[1])
        {
            if !self.view.scroll_down_button.disabled {
                let row = (self.view.scroll_rows + page).min(self.view.max_scroll_rows);
                self.strips.set_scroll_row(self.active_tab, row);
                self.rebuild_view(sim, rules);
            }
            return SidebarOutcome::Consumed;
        }
        if self.view.scroll_up_button.rect.contains(point[0], point[1]) {
            if !self.view.scroll_up_button.disabled {
                let row = self.view.scroll_rows.saturating_sub(page);
                self.strips.set_scroll_row(self.active_tab, row);
                self.rebuild_view(sim, rules);
            }
            return SidebarOutcome::Consumed;
        }
        let Some(item) = self
            .view
            .items
            .iter()
            .find(|item| item.cameo_rect().contains(point[0], point[1]))
            .cloned()
        else {
            return SidebarOutcome::Consumed;
        };
        if item.is_superweapon {
            return SidebarOutcome::Consumed;
        }
        let Some(owner) = sim.session().current_house() else {
            return SidebarOutcome::Consumed;
        };
        let Some(type_id) = sim.interner().get(&item.type_id) else {
            return SidebarOutcome::Consumed;
        };
        let Some(object) = sim.object_type(type_id, rules) else {
            return SidebarOutcome::Consumed;
        };
        let category = category_for_object(object);
        let owner_name = sim.interner().resolve(owner);
        let queue = queue_view_for_owner(sim, rules, owner_name);
        let linked = self.linked_factory(sim, owner, type_id);
        let own_state = linked.as_ref().map(|factory| factory.cameo_state());
        let request = self.strips.request_state(CameoId::Object(type_id));
        let press = if right {
            CameoPress::Right { shift }
        } else {
            CameoPress::Left
        };
        let click = cameo_click(
            &queue,
            category,
            Some(type_id),
            own_state,
            request,
            press,
            item.at_build_limit,
        );
        if click.clear_building_placement {
            self.armed_building = None;
        }
        self.record_request(type_id, click, sim);
        match click.order {
            Some(CameoOrder::Produce) => {
                SidebarOutcome::Command(crate::sim::command::Command::QueueProduction { type_id })
            }
            Some(CameoOrder::Suspend) => {
                SidebarOutcome::Command(crate::sim::command::Command::SuspendProduction {
                    category,
                })
            }
            Some(CameoOrder::Abandon { all }) => {
                SidebarOutcome::Command(crate::sim::command::Command::CancelProductionByType {
                    type_id,
                    all,
                })
            }
            Some(CameoOrder::Place) => {
                let Some(factory) = linked else {
                    return SidebarOutcome::Consumed;
                };
                let Some(product) = factory.complete_object() else {
                    return SidebarOutcome::Consumed;
                };
                let product_type = product.type_id;
                let is_structure = product
                    .entity_id
                    .and_then(|id| sim.entities().get(id))
                    .is_some_and(|entity| {
                        entity.category() == crate::map::entities::EntityCategory::Structure
                    })
                    || sim.object_type(product_type, rules).is_some_and(|object| {
                        object.category == crate::rules::object_type::ObjectCategory::Building
                    });
                if is_structure {
                    self.gadgets.repair_mode_on = false;
                    self.gadgets.sell_mode_on = false;
                    self.armed_building = Some(product_type);
                    self.strips.clear_request(CameoId::Object(type_id));
                    self.rebuild_view(sim, rules);
                    SidebarOutcome::ArmBuildingPlacement(product_type)
                } else {
                    SidebarOutcome::Command(crate::sim::command::Command::PlaceProducedMobile {
                        category,
                    })
                }
            }
            None => SidebarOutcome::Consumed,
        }
    }

    pub fn repair_mode(&self) -> bool {
        self.gadgets.repair_mode_on
    }

    pub fn sell_mode(&self) -> bool {
        self.gadgets.sell_mode_on
    }

    pub fn active_tab(&self) -> sidebar::SidebarTab {
        self.active_tab
    }

    pub fn clear_armed_building(&mut self) {
        self.armed_building = None;
    }

    fn record_request(&mut self, type_id: InternedId, click: CameoClick, sim: &Simulation) {
        let id = CameoId::Object(type_id);
        match click.order {
            Some(CameoOrder::Produce) => {
                self.strips
                    .start_request(id, sim.session().binary_frame() as i32, None);
            }
            Some(CameoOrder::Suspend) => self.strips.hold_request(id),
            Some(CameoOrder::Abandon { .. }) | Some(CameoOrder::Place) | None => {}
        }
    }

    fn linked_factory<'a>(
        &self,
        sim: &'a Simulation,
        owner: InternedId,
        type_id: InternedId,
    ) -> Option<crate::sim::production::FactoryView<'a>> {
        let link = self
            .factory_links
            .get(&type_id)
            .filter(|link| link.owner == owner)?;
        let factory = sim.production().factories().view(owner, link.category)?;
        (factory.construction_order() == link.construction_order).then_some(factory)
    }

    fn rebuild_view(&mut self, sim: &Simulation, rules: &RuleSet) {
        let Some(owner) = sim.session().current_house() else {
            return;
        };
        let Some(atlas) = self.chrome.for_theme(self.theme) else {
            return;
        };
        if let Ok(view) = build_view(
            sim,
            rules,
            owner,
            &self.strips,
            self.active_tab,
            atlas,
            self.theme,
            &self.gadgets,
            self.armed_building,
            &self.factory_links,
        ) {
            self.view = view;
        }
    }

    pub fn build_instances(
        &self,
        gpu: &GpuContext,
        pool: &mut InstanceBufferPool,
        camera: [f32; 2],
    ) {
        let spec = sidebar::SidebarChromeLayoutSpec::for_theme(self.theme);
        let atlas = self
            .chrome
            .for_theme(self.theme)
            .expect("sidebar atlas selected during load");
        let mut chrome = sidebar_chrome_instances(atlas, spec, &self.view);
        let mut cameo = Vec::new();
        let mut gclock = Vec::new();
        let mut status = Vec::new();
        if let Some(atlas) = &self.cameos {
            for item in &self.view.items {
                if let Some(entry) = atlas.cameo(&item.type_id, item.alt_cameo) {
                    if let Some(rect) = place_canvas_crop_in_slot(item.rect, entry) {
                        cameo.push(SpriteInstance {
                            position: [rect.x, rect.y],
                            size: [rect.w, rect.h],
                            uv_origin: entry.uv_origin,
                            uv_size: entry.uv_size,
                            depth: 0.00044,
                            tint: [1.0, 1.0, 1.0],
                            alpha: 1.0,
                            ..Default::default()
                        });
                    }
                }
                if !item.is_ready
                    && (item.is_building_this_type || item.is_on_hold || item.progress > 0.0)
                    && let Some(entry) = self.chrome.for_theme(self.theme).and_then(|chrome| {
                        let last = chrome.gclock_frames.len().checked_sub(1)?;
                        let stage = (item.progress.clamp(0.0, 1.0)
                            * f32::from(crate::sim::production::PRODUCTION_STEPS))
                        .round() as usize;
                        chrome.gclock_frames.get((stage + 1).min(last))
                    })
                {
                    gclock.push(SpriteInstance {
                        position: [item.rect.x, item.rect.y],
                        size: [item.rect.w, item.rect.h],
                        uv_origin: entry.uv_origin,
                        uv_size: entry.uv_size,
                        depth: 0.00043,
                        tint: [1.0, 1.0, 1.0],
                        alpha: 1.0,
                        draw_state: crate::render::draw_state::DrawState::from_native_selector(
                            0, 4,
                        ),
                        ..Default::default()
                    });
                }
                let label = if item.is_ready {
                    Some("READY")
                } else if item.is_on_hold {
                    Some("HOLD")
                } else {
                    None
                };
                if let Some(label) = label {
                    let width = self.font.text_width(label) as f32;
                    status.extend(self.font.build_text(
                        label,
                        item.rect.x + (item.rect.w - width) * 0.5,
                        item.rect.y + 1.0,
                        1.0,
                        0.00042,
                        sidebar_text::side_highlight_color(self.theme),
                        [0.0; 2],
                    ));
                }
            }
        }
        let mut text = sidebar_text::build_credits_instances(
            &self.font,
            self.view.credits,
            self.view.panel_rect,
            1.0,
            sidebar_text::credits_tint(self.theme),
            [0.0; 2],
        );
        if let Some(item) = self.view.items.first() {
            text.extend(self.font.build_text(
                &item.display_name,
                item.rect.x,
                item.rect.y + item.rect.h - self.font.cell_height(),
                1.0,
                0.00042,
                sidebar_text::side_highlight_color(self.theme),
                [0.0; 2],
            ));
        }
        text.extend(status);
        // BatchRenderer's UI camera retains the rounded world-camera position.
        // Its screen-fixed callers pre-offset instances by that same scroll.
        for instance in chrome
            .iter_mut()
            .chain(&mut cameo)
            .chain(&mut gclock)
            .chain(&mut text)
        {
            instance.position[0] += camera[0].round();
            instance.position[1] += camera[1].round();
        }
        pool.upload_page(gpu, "switch-sidebar-chrome", 0, &chrome);
        pool.upload_page(gpu, "switch-sidebar-cameo", 0, &cameo);
        pool.upload_page(gpu, "switch-sidebar-gclock", 0, &gclock);
        pool.upload_page(gpu, "switch-sidebar-text", 0, &text);
        log::info!(
            "switch_sidebar_instances tab={:?} credits={} items={} chrome={} cameo={} gclock={} text={}",
            self.active_tab,
            self.view.credits,
            self.view.items.len(),
            chrome.len(),
            cameo.len(),
            gclock.len(),
            text.len()
        );
    }

    pub fn draw<'a>(
        &'a self,
        batch: &'a BatchRenderer,
        pool: &'a InstanceBufferPool,
        pass: &mut wgpu::RenderPass<'a>,
    ) {
        if let Some((buffer, count)) = pool.get("switch-sidebar-chrome") {
            let atlas = self
                .chrome
                .for_theme(self.theme)
                .expect("sidebar atlas selected during load");
            batch.draw_with_buffer_ui_passthrough(pass, &atlas.texture, buffer, count);
        }
        if let Some(atlas) = &self.cameos
            && let Some((buffer, count)) = pool.get("switch-sidebar-cameo")
        {
            batch.draw_with_buffer_ui_passthrough(pass, &atlas.texture, buffer, count);
        }
        if let Some((buffer, count)) = pool.get("switch-sidebar-gclock")
            && let Some(texture) = self
                .chrome
                .for_theme(self.theme)
                .and_then(|atlas| atlas.gclock_texture.as_ref())
        {
            batch.draw_with_buffer_ui_passthrough(pass, texture, buffer, count);
        }
        if let Some((buffer, count)) = pool.get("switch-sidebar-text") {
            batch.draw_with_buffer_ui_passthrough(pass, self.font.atlas(), buffer, count);
        }
    }

    pub fn view(&self) -> &sidebar::SidebarView {
        &self.view
    }

    pub fn counts(&self, pool: &InstanceBufferPool) -> (usize, usize, usize, usize, usize) {
        let count = |key| pool.get(key).map_or(0, |(_, count)| count as usize);
        (
            self.view.items.len(),
            count("switch-sidebar-chrome"),
            count("switch-sidebar-cameo"),
            count("switch-sidebar-text"),
            count("switch-sidebar-gclock"),
        )
    }

    pub fn generated_font(&self) -> bool {
        false
    }
}

fn build_view(
    sim: &Simulation,
    rules: &RuleSet,
    owner: crate::sim::intern::InternedId,
    strips: &CameoStrips,
    active_tab: sidebar::SidebarTab,
    atlas: &SidebarChromeAtlas,
    theme: SidebarTheme,
    gadgets: &SidebarGadgetState,
    armed_building: Option<InternedId>,
    factory_links: &HashMap<InternedId, CameoFactoryLink>,
) -> Result<sidebar::SidebarView> {
    let owner_name = sim.interner().resolve(owner);
    let options = all_build_options_for_owner(sim, rules, owner_name);
    let queue = queue_view_for_owner(sim, rules, owner_name);
    let ready = ready_buildings_for_owner(sim, rules, owner_name);
    let focus: Vec<_> = [
        ProductionCategory::Building,
        ProductionCategory::Defense,
        ProductionCategory::Infantry,
        ProductionCategory::Vehicle,
        ProductionCategory::Aircraft,
        ProductionCategory::Ship,
    ]
    .into_iter()
    .filter_map(|category| active_producer_for_owner_category(sim, rules, owner_name, category))
    .collect();
    let supers = superweapon_views_for_owner(sim, rules, &owner);
    let (power_produced, power_drained) = power_balance_for_owner(sim, rules, owner_name);
    let mut view = sidebar::build_sidebar_view_for_frontend(
        sidebar::SidebarChromeLayoutSpec::for_theme(theme),
        1280.0,
        720.0,
        active_tab,
        credits_for_owner(sim, owner_name),
        power_produced,
        power_drained,
        atlas.tab_frames[0][0].map(|e| e.pixel_size),
        &queue,
        &options,
        &ready,
        None,
        &focus,
        Some(sim.interner()),
        &supers,
        gadgets,
        atlas.repair_frames[0].map(|e| e.pixel_size),
        atlas.sell_frames[0].map(|e| e.pixel_size),
        atlas.scroll_down_frames[0].map(|e| e.pixel_size),
        atlas.scroll_up_frames[0].map(|e| e.pixel_size),
        [
            atlas.top_button_frames[0][0].map(|e| e.pixel_size),
            atlas.top_button_frames[1][0].map(|e| e.pixel_size),
        ],
        strips,
    );
    apply_item_state(
        &mut view,
        sim,
        rules,
        owner,
        strips,
        armed_building,
        factory_links,
    );
    Ok(view)
}

fn apply_item_state(
    view: &mut sidebar::SidebarView,
    sim: &Simulation,
    rules: &RuleSet,
    owner: InternedId,
    strips: &CameoStrips,
    armed_building: Option<InternedId>,
    factory_links: &HashMap<InternedId, CameoFactoryLink>,
) {
    let mut busy = [false; 4];
    for tab in sidebar::SidebarTab::all() {
        busy[tab.tab_index()] = strips.items(tab).iter().any(|cameo| {
            let CameoId::Object(type_id) = cameo.id else {
                return false;
            };
            let Some(object) = sim.object_type(type_id, rules) else {
                return false;
            };
            let category = category_for_object(object);
            linked_factory_for_type(sim, owner, type_id, factory_links)
                .is_some_and(|factory| factory.object.is_some())
        });
    }
    for item in &mut view.items {
        if item.is_superweapon {
            continue;
        }
        let (Some(type_id), Some(object)) = (
            sim.interner().get(&item.type_id),
            rules.object(&item.type_id),
        ) else {
            continue;
        };
        let category = category_for_object(object);
        let factory = linked_factory_for_type(sim, owner, type_id, factory_links);
        let status = factory.as_ref().map(|factory| {
            let is_ready = factory.complete_object().is_some();
            crate::ui_sidebar::CameoFactoryStatus {
                type_id,
                progress: factory.progress,
                is_ready,
                is_building: !is_ready && factory.is_running(),
                is_on_hold: !is_ready && !factory.is_running(),
            }
        });
        let tab = sidebar::SidebarTab::for_category(category);
        crate::ui_sidebar::draw_object_cameo(
            item,
            &crate::ui_sidebar::ObjectCameoDrawInputs {
                linked: factory.is_some(),
                request: strips.request_state(CameoId::Object(type_id)),
                draw: cameo_draw_state(sim, rules, owner, object),
                strip_blocks: object.category
                    == crate::rules::object_type::ObjectCategory::Building
                    && busy[tab.tab_index()],
                status,
                armed: armed_building == Some(type_id),
            },
        );
    }
}

fn linked_factory_for_type<'a>(
    sim: &'a Simulation,
    owner: InternedId,
    type_id: InternedId,
    factory_links: &HashMap<InternedId, CameoFactoryLink>,
) -> Option<crate::sim::production::FactoryView<'a>> {
    let link = factory_links
        .get(&type_id)
        .filter(|link| link.owner == owner)?;
    let factory = sim.production().factories().view(owner, link.category)?;
    (factory.construction_order() == link.construction_order).then_some(factory)
}

fn seed_factory_links(
    sim: &Simulation,
    owner: InternedId,
    strips: &CameoStrips,
) -> HashMap<InternedId, CameoFactoryLink> {
    let entries = strips.saved_identities();
    let mut links = HashMap::new();
    for category in [
        ProductionCategory::Building,
        ProductionCategory::Defense,
        ProductionCategory::Infantry,
        ProductionCategory::Vehicle,
        ProductionCategory::Aircraft,
        ProductionCategory::Ship,
    ] {
        let Some(factory) = sim.production().factories().view(owner, category) else {
            continue;
        };
        let Some(object) = factory.object else {
            continue;
        };
        if entries
            .iter()
            .flatten()
            .any(|id| *id == CameoId::Object(object.type_id))
        {
            links.insert(
                object.type_id,
                CameoFactoryLink {
                    owner,
                    category,
                    construction_order: factory.construction_order(),
                },
            );
        }
    }
    links
}

fn candidates(
    sim: &Simulation,
    rules: &RuleSet,
    owner: crate::sim::intern::InternedId,
) -> Vec<Cameo> {
    let owner_name = sim.interner().resolve(owner);
    let mut out = Vec::new();
    for option in all_build_options_for_owner(sim, rules, owner_name) {
        let Some(object) = sim.interner().object_type(option.type_id, rules) else {
            continue;
        };
        out.push(Cameo {
            id: CameoId::Object(option.type_id),
            tab: sidebar::SidebarTab::for_category(crate::sim::production::category_for_object(
                object,
            )),
            available: true,
            key: CameoKey::Techno {
                own_side: true,
                considered_aircraft: false,
                naval: object.naval,
                tech_level: 0,
                cost: option.cost,
                name: option.display_name,
            },
        });
    }
    for ready in ready_buildings_for_owner(sim, rules, owner_name) {
        if out.iter().any(|c| c.id == CameoId::Object(ready.type_id)) {
            continue;
        }
        let Some(object) = sim.interner().object_type(ready.type_id, rules) else {
            continue;
        };
        out.push(Cameo {
            id: CameoId::Object(ready.type_id),
            tab: sidebar::SidebarTab::for_category(crate::sim::production::category_for_object(
                object,
            )),
            available: true,
            key: CameoKey::Techno {
                own_side: true,
                considered_aircraft: false,
                naval: object.naval,
                tech_level: 0,
                cost: 0,
                name: ready.display_name,
            },
        });
    }
    for sw in superweapon_views_for_owner(sim, rules, &owner) {
        out.push(Cameo {
            id: CameoId::SuperWeapon(sw.type_id),
            tab: sidebar::SidebarTab::Defense,
            available: true,
            key: CameoKey::SuperWeapon {
                recharge_frames: 0,
                name: sw.display_name,
            },
        });
    }
    out
}

fn sidebar_chrome_instances(
    atlas: &SidebarChromeAtlas,
    mut spec: sidebar::SidebarChromeLayoutSpec,
    view: &sidebar::SidebarView,
) -> Vec<SpriteInstance> {
    spec.side2_height = atlas.side2.pixel_size[1];
    spec.side3_height = atlas.side3.pixel_size[1];
    build_chrome(atlas, spec, view)
}

fn build_chrome(
    atlas: &SidebarChromeAtlas,
    spec: sidebar::SidebarChromeLayoutSpec,
    view: &sidebar::SidebarView,
) -> Vec<SpriteInstance> {
    let mut out = Vec::new();
    let d = 0.00048;
    let cx = view.layout.sidebar_x;
    let push = |out: &mut Vec<SpriteInstance>,
                entry: crate::render::sidebar_chrome::SidebarChromeEntry,
                x: f32,
                y: f32| {
        out.push(SpriteInstance {
            position: [x, y],
            size: entry.pixel_size,
            uv_origin: entry.uv_origin,
            uv_size: entry.uv_size,
            depth: d,
            tint: [1.0, 1.0, 1.0],
            alpha: 1.0,
            ..Default::default()
        });
    };
    if let Some(entry) = atlas.top_strip_sidebar {
        push(&mut out, entry, cx, 16.0);
    }
    if let Some(entry) = atlas.top_strip_thin {
        push(&mut out, entry, cx, 0.0);
    }
    push(&mut out, atlas.radar, cx, view.layout.radar_y);
    push(&mut out, atlas.side1, cx, view.layout.side1_y);
    for tab in &view.tabs {
        let entry = atlas.tab_frames[tab.tab.tab_index()][tab.frame_index as usize]
            .or(atlas.tab_frames[tab.tab.tab_index()][0]);
        if let Some(entry) = entry {
            push(&mut out, entry, tab.rect.x, tab.rect.y);
        }
    }
    for row in 0..view.layout.side2_tile_count {
        let y = view.layout.cameo_grid_top + row as f32 * atlas.side2.pixel_size[1];
        push(&mut out, atlas.side2, cx, y);
    }
    push(&mut out, atlas.side3, cx, view.layout.side3_y);
    if let Some(entry) = atlas.unknown_top_housing {
        push(&mut out, entry, cx, view.layout.side3_y);
    }
    if let Some(entry) = atlas.unknown_mid_panel {
        push(&mut out, entry, cx, view.layout.side3_y);
    }
    let repair =
        atlas.repair_frames[view.repair_button.frame_index as usize].or(atlas.repair_frames[0]);
    if let Some(entry) = repair {
        push(
            &mut out,
            entry,
            view.repair_button.rect.x,
            view.repair_button.rect.y,
        );
    }
    let sell = atlas.sell_frames[view.sell_button.frame_index as usize].or(atlas.sell_frames[0]);
    if let Some(entry) = sell {
        push(
            &mut out,
            entry,
            view.sell_button.rect.x,
            view.sell_button.rect.y,
        );
    }
    let _ = spec;
    out
}

const VISIBLE_ROWS: usize = 8;

fn apply_transitions(
    transitions: crate::ui_sidebar::cameo_order::CameoTransitions,
    active_tab: &mut sidebar::SidebarTab,
    gadgets: &mut SidebarGadgetState,
    strips: &CameoStrips,
) {
    for tab in sidebar::SidebarTab::all() {
        let disabled = transitions
            .tab_button_disabled(tab)
            .unwrap_or(strips.items(tab).is_empty());
        gadgets.set_tab_disabled(tab.tab_index(), disabled);
    }
    if let Some(destination) = transitions.active_tab_destination() {
        *active_tab = destination;
    }
}

fn place_canvas_crop_in_slot(
    slot: sidebar::Rect,
    entry: &crate::render::sidebar_cameo_atlas::SidebarCameoEntry,
) -> Option<sidebar::Rect> {
    let [canvas_w, canvas_h] = entry.canvas_size;
    let [crop_w, crop_h] = entry.pixel_size;
    if canvas_w <= 0.0 || canvas_h <= 0.0 || crop_w <= 0.0 || crop_h <= 0.0 {
        return None;
    }
    let scale = (slot.w / canvas_w).min(slot.h / canvas_h);
    let canvas_x = slot.x + (slot.w - canvas_w * scale) * 0.5;
    let canvas_y = slot.y + (slot.h - canvas_h * scale) * 0.5;
    let left = (canvas_x + entry.crop_origin[0] * scale).round();
    let top = (canvas_y + entry.crop_origin[1] * scale).round();
    let right = (canvas_x + (entry.crop_origin[0] + crop_w) * scale).round();
    let bottom = (canvas_y + (entry.crop_origin[1] + crop_h) * scale).round();
    Some(sidebar::Rect {
        x: left,
        y: top,
        w: right - left,
        h: bottom - top,
    })
}
