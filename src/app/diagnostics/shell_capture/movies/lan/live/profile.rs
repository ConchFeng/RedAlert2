//! Host-only actual page/combo/Back/recreate proof for process-retained profiles.
//! No member, slot choice or profile snapshot is injected into the owner.
use super::*;

#[derive(Default)]
pub(super) struct ProfileCheck {
    choice: usize,
    popup: Option<(u16, usize, i32)>,
    before: Option<Value>,
    restored: Option<Value>,
    backed: bool,
    reconstructing: bool,
}
pub(super) enum ProfileStep {
    Ordinary,
    Waiting,
    Recreate,
    Exit,
}

impl ProfileCheck {
    pub(super) fn started(&self) -> bool {
        self.before.is_some() || self.choice != 0 || self.popup.is_some()
    }
    pub(super) fn receipt(&self) -> Result<Value> {
        ensure!(
            self.before.is_some() && self.restored.is_some(),
            "extended profile capture incomplete"
        );
        Ok(
            json!({"process_id":std::process::id(),"before":self.before,"restored":self.restored,
            "input":"normal roster page buttons and presented combo popups, Back, Create, readback",
            "scope":"host-only process profile retention; no match or30-human acceptance claim"}),
        )
    }
    pub(super) fn step(
        &mut self,
        state: &mut AppState,
        rendered: PresentedShell,
        online: bool,
        frame: u32,
    ) -> Result<ProfileStep> {
        let lobby = if online {
            rendered == PresentedShell::WolLobby && online::online_lobby_settled(state)
        } else {
            rendered == PresentedShell::LanLobby && lobby_settled(state)
        };
        if self.backed && lobby {
            self.backed = false;
            if self.restored.is_some() {
                return Ok(ProfileStep::Exit);
            }
            self.reconstructing = true;
            return Ok(ProfileStep::Recreate);
        }
        if rendered != PresentedShell::NetworkSetup
            || !(if online {
                online::online_setup_settled(state, true)
            } else {
                setup_settled(state, true)
            })
        {
            return Ok(if self.backed {
                ProfileStep::Waiting
            } else {
                ProfileStep::Ordinary
            });
        }
        ensure!(
            state
                .match_state
                .session
                .network_setup()
                .is_some_and(|launch| launch.humans.len() == 1)
                && state
                    .match_state
                    .session
                    .network_map_metadata()
                    .is_some_and(|meta| meta.player_capacity >= 30),
            "profile check needs the host alone on an admitted30-capable map"
        );
        if self.reconstructing {
            let restored = snapshot(state, frame)?;
            let before = self.before.as_ref().context("remembered profile")?;
            ensure!(
                restored["map"] == before["map"]
                    && restored["rows"]
                        .as_array()
                        .zip(before["rows"].as_array())
                        .is_some_and(|(after, before)| after[..8] == before[..8]
                            && after[24] == before[24]
                            && after[29]["vacancy"] == before[29]["vacancy"]),
                "Back/Create lost extended or native setup choices"
            );
            self.restored = Some(restored);
            setup_button(state, crate::ui::multiplayer_setup::BACK)?;
            self.backed = true;
            self.reconstructing = false;
            return Ok(ProfileStep::Waiting);
        }
        if self.before.is_some() {
            return Ok(ProfileStep::Waiting);
        }
        let choices = [
            (SetupColumn::Player, 29, 3),
            (SetupColumn::Player, 24, 6),
            (SetupColumn::Country, 24, 0),
            (SetupColumn::Color, 24, 1),
        ];
        if let Some((control, index, value)) = self.popup.take() {
            let view = setup::view(state).context("presented profile combo")?;
            let layout = setup_input::layout(state);
            let ui = state
                .frontend
                .multiplayer_setup
                .as_ref()
                .context("profile popup owner")?;
            ensure!(
                ui.popup.control == Some(control)
                    && view
                        .combo(control)
                        .and_then(|combo| combo.choices.get(index))
                        .is_some_and(|choice| choice.value == value),
                "profile popup changed before its rendered choice click"
            );
            let popup = ui
                .popup_geometry(&view, &layout)
                .context("presented profile popup geometry")?;
            let Some(rect) = popup.row_rect(index) else {
                // Player shows only three rows. Scroll the retained absolute
                // item into view through the normal input owner, then require
                // a new presentation before selecting it.
                let geometry = popup.scroll_geometry();
                let bar = geometry.scrollbar.context("profile popup scrollbar")?;
                let down = index >= popup.first + popup.visible_rows;
                let (x, y) =
                    crate::ui::shell::list::scrollbar_arrow_origins(bar, geometry.metrics.arrow_h)
                        [usize::from(down)];
                setup_click(
                    state,
                    RectPx::new(x, y, bar.w - 2, geometry.metrics.arrow_h),
                );
                ensure!(
                    state.frontend.multiplayer_setup.as_ref().is_some_and(|ui| {
                        ui.popup.control == Some(control)
                            && if down {
                                ui.popup.top_index > popup.first
                            } else {
                                ui.popup.top_index < popup.first
                            }
                    }),
                    "profile popup scrollbar did not advance toward the retained item"
                );
                self.popup = Some((control, index, value));
                return Ok(ProfileStep::Waiting);
            };
            setup_click(state, rect);
            ensure!(
                state
                    .frontend
                    .multiplayer_setup
                    .as_ref()
                    .is_some_and(|ui| ui.popup.control.is_none()),
                "profile combo input did not close popup"
            );
            self.choice += 1;
            return Ok(ProfileStep::Waiting);
        }
        if let Some(&(column, row, value)) = choices.get(self.choice) {
            let control =
                setup_control_at_row(state, column, row)?.context("profile visible raw row")?;
            let view = setup::view(state).context("profile setup")?;
            let combo = view.combo(control).context("enabled profile combo")?;
            ensure!(
                combo.enabled && combo.visible,
                "profile combo is not interactive"
            );
            let index = combo
                .choices
                .iter()
                .position(|choice| choice.value == value)
                .context("profile legal item")?;
            let layout = setup_input::layout(state);
            let face = layout.control_rect(control).context("profile combo face")?;
            setup_click(
                state,
                crate::ui::skirmish_shell::combo_arrow_hit_rect(face, layout.page.scale),
            );
            ensure!(
                state
                    .frontend
                    .multiplayer_setup
                    .as_ref()
                    .is_some_and(|ui| ui.popup.control == Some(control)),
                "actual profile combo arrow did not open"
            );
            self.popup = Some((control, index, value));
            return Ok(ProfileStep::Waiting); // Require an actual popup presentation.
        }
        let session = &state.match_state.session;
        ensure!(
            session.network_vacancy(29)
                == Some(crate::app::match_runtime::session::NetworkVacancy::Closed)
                && session.network_display_ai(24)
                    == Some(crate::skirmish_launch::AiDifficulty::Hard)
                && session
                    .network_row_choices(24)
                    .is_some_and(|slot| !slot.country_random
                        && !slot.color_random
                        && slot.color_index == 1),
            "actual extended combo choices were not applied"
        );
        self.before = Some(snapshot(state, frame)?);
        setup_button(state, crate::ui::multiplayer_setup::BACK)?;
        self.backed = true;
        Ok(ProfileStep::Waiting)
    }
}
fn snapshot(state: &AppState, frame: u32) -> Result<Value> {
    let session = &state.match_state.session;
    let launch = session.network_setup().context("profile setup snapshot")?;
    Ok(json!({"render_frame":frame,"process_id":std::process::id(),
        "capacity":session.network_map_metadata().context("profile map metadata")?.player_capacity,
        "map":launch.selected_map_file,"rows":(0..30).map(|row|json!({
        "row":row,"vacancy":session.network_vacancy(row).map(|v|format!("{v:?}")),
        "ai":session.network_display_ai(row).map(|v|format!("{v:?}")),"slot":session.network_row_choices(row)
    })).collect::<Vec<_>>() }))
}
