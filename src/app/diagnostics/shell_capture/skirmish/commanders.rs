//! The commander panel's production input and persistence capture route.

use super::*;
use crate::app::frontend::ai_commanders_render;
use crate::ui::ai_commanders::AiCommanderControl;

#[derive(Default)]
enum Step {
    #[default]
    Browse,
    ReturnToFirst,
    Choose,
    Apply,
    NamedRow,
    NamedChoice,
    Commit,
    Reopen,
    Done,
}

#[derive(Default)]
pub(super) struct CommanderCapture {
    step: Step,
    visited: Vec<usize>,
    rng: Option<crate::sim::rng::SimRngLogicalState>,
    expected_choices: Vec<(usize, String)>,
    expected_feedback: bool,
    presented: bool,
    surface: (u32, u32),
    named: Option<(usize, String)>,
    settled_at: Option<Instant>,
}

fn click(state: &mut AppState, point: (i32, i32)) -> Result<()> {
    SkirmishCapture::set_pointer(state, point);
    crate::app::input::ai_commanders::cursor_moved(state);
    for pressed in [true, false] {
        ensure!(
            crate::app::input::ai_commanders::mouse(
                state,
                winit::event::MouseButton::Left,
                pressed,
            ),
            "commander input owner did not capture its visible control"
        );
    }
    SkirmishCapture::set_pointer(state, (EXPECTED_CURSOR_X as i32, EXPECTED_CURSOR_Y as i32));
    crate::app::input::ai_commanders::cursor_moved(state);
    Ok(())
}

fn activate(state: &mut AppState, control: AiCommanderControl) -> Result<()> {
    let layout = ai_commanders_render::layout(state);
    let setup = state
        .frontend
        .ai_commanders_setup
        .as_ref()
        .context("commander panel closed before its control was activated")?;
    let rect = layout
        .control_rect(control)
        .context("commander control is hidden")?;
    let point = (rect.x + rect.w / 2, rect.y + rect.h / 2);
    ensure!(
        layout.control_at(setup, point) == Some(control),
        "commander controls overlap or the requested control is disabled"
    );
    click(state, point)
}

fn open_panel(state: &mut AppState) -> Result<()> {
    let rect = ai_commanders_render::entry_rect(state)
        .context("AI Commanders entry is unavailable after accepted observer setup")?;
    click(state, (rect.x + rect.w / 2, rect.y + rect.h / 2))?;
    ensure!(
        state.frontend.ai_commanders_setup.is_some(),
        "commander panel did not open"
    );
    Ok(())
}

impl CommanderCapture {
    pub(super) fn new(surface: (u32, u32)) -> Self {
        Self {
            surface,
            ..Self::default()
        }
    }

    pub(super) fn surface(&self) -> (u32, u32) {
        self.surface
    }

    pub(super) fn open(
        &mut self,
        state: &mut AppState,
        route: &mut Vec<Value>,
        frame: u32,
    ) -> Result<()> {
        ensure!(
            state.match_state.sim_runtime.is_none(),
            "setup unexpectedly owns a simulation"
        );
        self.rng = Some(
            state
                .frontend
                .multiplayer_shell_runtime
                .scenario_rng_logical_state(),
        );
        open_panel(state)?;
        let setup = state.frontend.ai_commanders_setup.as_ref().unwrap();
        ensure!(
            setup.rows.len() == 29,
            "commander panel omitted active AI entrants"
        );
        route.push(json!({"screen":"AI Commanders", "frame":frame, "action":"OpenAiCommanders", "entrants":setup.rows.len(), "custom_choices":setup.has_custom_choices()}));
        self.validate_unchanged(state)
    }

    fn validate_unchanged(&self, state: &AppState) -> Result<()> {
        ensure!(
            state.match_state.sim_runtime.is_none(),
            "commander setup created a simulation"
        );
        ensure!(
            self.rng.as_ref()
                == Some(
                    &state
                        .frontend
                        .multiplayer_shell_runtime
                        .scenario_rng_logical_state()
                ),
            "commander setup consumed gameplay RNG"
        );
        Ok(())
    }

    pub(super) fn after_present(
        &mut self,
        state: &mut AppState,
        route: &mut Vec<Value>,
        frame: u32,
    ) -> Result<()> {
        self.validate_unchanged(state)?;
        let rows = ai_commanders_render::layout(state).rows.len();
        match self.step {
            Step::Browse => {
                let setup = state
                    .frontend
                    .ai_commanders_setup
                    .as_ref()
                    .context("commander panel missing")?;
                let slots: Vec<_> = setup
                    .rows
                    .iter()
                    .skip(setup.page_top)
                    .take(rows)
                    .map(|row| row.slot_index)
                    .collect();
                for slot in &slots {
                    ensure!(
                        !self.visited.contains(slot),
                        "commander paging repeated an entrant"
                    );
                    self.visited.push(*slot);
                }
                route.push(json!({"screen":"AI Commanders", "frame":frame, "action":"PresentedCommanderPage", "slots":slots}));
                if setup.enabled(AiCommanderControl::Next, rows) {
                    activate(state, AiCommanderControl::Next)?;
                } else {
                    ensure!(
                        self.visited == (0..29).collect::<Vec<_>>(),
                        "commander paging did not visit all 29 entrants"
                    );
                    self.step = Step::ReturnToFirst;
                }
            }
            Step::ReturnToFirst => {
                if state
                    .frontend
                    .ai_commanders_setup
                    .as_ref()
                    .context("commander panel missing")?
                    .page_top
                    > 0
                {
                    activate(state, AiCommanderControl::Previous)?;
                } else {
                    activate(state, AiCommanderControl::Row(0))?;
                    self.step = Step::Choose;
                }
            }
            Step::Choose => {
                let setup = state
                    .frontend
                    .ai_commanders_setup
                    .as_ref()
                    .context("commander choices missing")?;
                ensure!(
                    setup
                        .choosing()
                        .and_then(|row| row.choices.first())
                        .is_some_and(|choice| choice.name == "Standard"),
                    "Standard is not the first compatible choice"
                );
                activate(state, AiCommanderControl::Choice(0))?;
                self.step = Step::Apply;
            }
            Step::Apply => {
                activate(state, AiCommanderControl::AllStandard)?;
                let setup = state
                    .frontend
                    .ai_commanders_setup
                    .as_ref()
                    .context("commander panel missing")?;
                ensure!(
                    setup.rows.iter().all(|row| row.selected == "Standard"),
                    "Apply All Standard omitted an entrant"
                );
                let random_available = setup.enabled(AiCommanderControl::AllRandom, rows);
                if random_available {
                    activate(state, AiCommanderControl::AllRandom)?;
                }
                self.named = state
                    .frontend
                    .ai_commanders_setup
                    .as_ref()
                    .unwrap()
                    .rows
                    .iter()
                    .enumerate()
                    .find_map(|(index, row)| {
                        row.choices
                            .iter()
                            .find(|choice| {
                                !["Standard", "Random"]
                                    .iter()
                                    .any(|reserved| choice.name.eq_ignore_ascii_case(reserved))
                            })
                            .map(|choice| (index, choice.name.clone()))
                    });
                route.push(json!({"screen":"AI Commanders", "frame":frame, "action":"ApplyAllCommanderChoices", "random_available":random_available}));
                self.step = if self.named.is_some() {
                    Step::NamedRow
                } else {
                    Step::Commit
                };
            }
            Step::NamedRow => {
                let index = self
                    .named
                    .as_ref()
                    .context("named commander fixture missing")?
                    .0;
                let top = state
                    .frontend
                    .ai_commanders_setup
                    .as_ref()
                    .context("commander panel missing")?
                    .page_top;
                if index >= top + rows {
                    activate(state, AiCommanderControl::Next)?;
                } else if index < top {
                    activate(state, AiCommanderControl::Previous)?;
                } else {
                    activate(state, AiCommanderControl::Row(index - top))?;
                    self.step = Step::NamedChoice;
                }
            }
            Step::NamedChoice => {
                let (index, name) = self
                    .named
                    .as_ref()
                    .context("named commander fixture missing")?;
                let setup = state
                    .frontend
                    .ai_commanders_setup
                    .as_ref()
                    .context("commander choices missing")?;
                let row = setup
                    .choosing()
                    .context("named commander choices did not open")?;
                let choice = row
                    .choices
                    .iter()
                    .position(|choice| choice.name == *name)
                    .context("named commander left its compatible list")?;
                if choice >= setup.choice_top + rows {
                    activate(state, AiCommanderControl::Next)?;
                } else {
                    let slot = row.slot_index;
                    let control = AiCommanderControl::Choice(choice - setup.choice_top);
                    activate(state, control)?;
                    ensure!(
                        state.frontend.ai_commanders_setup.as_ref().unwrap().rows[*index].selected
                            == *name,
                        "named commander was not selected through its visible choice"
                    );
                    route.push(json!({"screen":"AI Commanders", "frame":frame, "action":"SelectNamedCommander", "slot":slot, "commander":name}));
                    self.step = Step::Commit;
                }
            }
            Step::Commit => {
                let selected_before_feedback: Vec<_> = state
                    .frontend
                    .ai_commanders_setup
                    .as_ref()
                    .unwrap()
                    .rows
                    .iter()
                    .map(|row| (row.slot_index, row.selected.clone()))
                    .collect();
                let feedback_before = state
                    .frontend
                    .ai_commanders_setup
                    .as_ref()
                    .unwrap()
                    .feedback_enabled;
                activate(state, AiCommanderControl::Feedback)?;
                let setup = state.frontend.ai_commanders_setup.as_ref().unwrap();
                ensure!(
                    setup.feedback_enabled != feedback_before
                        && setup
                            .rows
                            .iter()
                            .map(|row| (row.slot_index, row.selected.clone()))
                            .collect::<Vec<_>>()
                            == selected_before_feedback,
                    "feedback switch changed commander strategy choices"
                );
                self.expected_choices = selected_before_feedback;
                self.expected_feedback = setup.feedback_enabled;
                activate(state, AiCommanderControl::Apply)?;
                ensure!(
                    state.frontend.ai_commanders_setup.is_none(),
                    "Apply left the commander panel open"
                );
                route.push(json!({"screen":"AI Commanders", "frame":frame, "action":"ApplyCommanderChoices", "feedback_enabled":self.expected_feedback, "choices":self.expected_choices, "gameplay_rng_unchanged":true, "simulation_created":false}));
                self.step = Step::Reopen;
            }
            Step::Reopen => {
                open_panel(state)?;
                let setup = state.frontend.ai_commanders_setup.as_ref().unwrap();
                ensure!(
                    setup.feedback_enabled == self.expected_feedback
                        && setup
                            .rows
                            .iter()
                            .map(|row| (row.slot_index, row.selected.clone()))
                            .collect::<Vec<_>>()
                            == self.expected_choices,
                    "commander choices or independent feedback setting were lost when reopening"
                );
                route.push(json!({"screen":"AI Commanders", "frame":frame, "action":"ReopenPersistedCommanderChoices", "entrants":setup.rows.len(), "feedback_enabled":setup.feedback_enabled, "gameplay_rng_unchanged":true}));
                self.settled_at = Some(Instant::now());
                self.step = Step::Done;
            }
            Step::Done => {
                self.presented = self
                    .settled_at
                    .is_some_and(|time| time.elapsed() >= std::time::Duration::from_millis(250));
            }
        }
        self.validate_unchanged(state)
    }

    pub(super) fn ready(&self, state: &AppState) -> bool {
        self.presented
            && matches!(self.step, Step::Done)
            && state.frontend.ai_commanders_setup.is_some()
    }
}
