//! Offline setup choices projected onto the existing personality catalog.

use crate::app::{AppState, GameScreen};
use crate::rules::personality::{PersonalityCatalog, PersonalityDef};
use crate::rules::ruleset::{CountryIdx, RuleSet};
use crate::skirmish_launch::SkirmishLaunchSession;
use crate::ui::ai_commanders::{
    AiCommanderChoice, AiCommanderRow, AiCommanderSetupEffect, AiCommanderSetupState,
};
use crate::ui::skirmish_shell::SkirmishShellState;

pub(crate) fn setup_available(state: &AppState) -> bool {
    state.frontend.screen == GameScreen::MainMenu
        && state.frontend.shell_route.skirmish()
        && state.frontend.ai_series.is_none()
        && state.rules().is_some()
}

fn localized(state: &AppState, key: &str, fallback: &str) -> String {
    state
        .process_assets
        .csf
        .as_ref()
        .and_then(|csf| csf.get(key))
        .filter(|text| !text.starts_with("MISSING:"))
        .unwrap_or(fallback)
        .to_owned()
}

fn country_name(rules: &RuleSet, country: crate::skirmish_launch::LaunchCountry) -> &str {
    rules
        .country_name(CountryIdx(country.raw_index()))
        .unwrap_or("")
}

fn admits_setup_country(
    definition: &PersonalityDef,
    rules: &RuleSet,
    shell: &SkirmishShellState,
    index: usize,
) -> bool {
    let Some(opponent) = shell.opponents.get(index) else {
        return false;
    };
    if !opponent.country_random {
        return definition.admits(country_name(rules, opponent.country));
    }
    definition.countries.is_none()
}

fn reconcile_choices(shell: &mut SkirmishShellState, rules: &RuleSet) -> Vec<usize> {
    let catalog = rules.personalities();
    let mut reset = Vec::new();
    for index in 0..shell.opponents.len() {
        let opponent = &shell.opponents[index];
        let selected = catalog
            .compatible_selection(country_name(rules, opponent.country), &opponent.commander);
        let compatible = selected.is_some_and(|name| {
            name == PersonalityCatalog::STANDARD
                || name == PersonalityCatalog::RANDOM
                || catalog
                    .index_of_name(name)
                    .and_then(|id| catalog.get(id))
                    .is_some_and(|definition| admits_setup_country(definition, rules, shell, index))
        });
        if compatible {
            shell.opponents[index].commander = selected.unwrap().to_owned();
        } else {
            shell.opponents[index].commander = PersonalityCatalog::STANDARD.to_owned();
            reset.push(index + 1);
        }
    }
    reset
}

pub(crate) fn reconcile(state: &mut AppState) {
    if !setup_available(state) || state.frontend.ai_commanders_setup.is_some() {
        return;
    }
    let Some(rules) = state.frontend.frontend_rules.as_ref() else {
        return;
    };
    let reset = reconcile_choices(&mut state.frontend.skirmish_shell_state, rules);
    if !reset.is_empty() {
        state.frontend.ai_commanders_notice = Some(format!(
            "Computer {} reset to Standard: the saved commander is missing or incompatible with the country selection.",
            reset
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
}

fn strategy_summary(definition: &PersonalityDef) -> String {
    let preferences = definition.deny.len() + definition.trigger_weight_percent.len();
    if definition.trigger_ratio == 100 && definition.team_delay_percent == 100 && preferences == 0 {
        return "Feedback only; standard AI tactics.".to_owned();
    }
    format!(
        "Team delay {}%; trigger chance {}%; {} configured trigger preferences.",
        definition.team_delay_percent, definition.trigger_ratio, preferences
    )
}

pub(crate) fn open_setup(state: &mut AppState) {
    if !setup_available(state) || state.frontend.ai_series_setup.is_some() {
        return;
    }
    reconcile(state);
    let Some(rules) = state.rules() else {
        return;
    };
    let catalog = rules.personalities();
    let shell = &state.frontend.skirmish_shell_state;
    let mut rows = Vec::new();
    for (index, opponent) in shell
        .opponents
        .iter()
        .enumerate()
        .filter(|(_, opponent)| opponent.is_active())
    {
        let mut choices = vec![AiCommanderChoice {
            name: PersonalityCatalog::STANDARD.to_owned(),
            label: localized(state, "VERA:CommanderStandard", "Standard"),
            description: "Standard AI behavior; no commander feedback.".to_owned(),
        }];
        if !catalog.is_empty() {
            choices.push(AiCommanderChoice {
                name: PersonalityCatalog::RANDOM.to_owned(),
                label: localized(state, "VERA:CommanderRandom", "Random compatible commander"),
                description:
                    "Chooses a compatible commander at launch; uses Standard if none are available."
                        .to_owned(),
            });
        }
        for id in catalog.candidates_for(country_name(rules, opponent.country)) {
            let Some(definition) = catalog
                .get(id)
                .filter(|definition| admits_setup_country(definition, rules, shell, index))
            else {
                continue;
            };
            choices.push(AiCommanderChoice {
                name: definition.name.clone(),
                label: definition
                    .display_name
                    .as_deref()
                    .map(|key| localized(state, key, &definition.name))
                    .unwrap_or_else(|| definition.name.clone()),
                description: strategy_summary(definition),
            });
        }
        let country = if opponent.country_random {
            "Random country".to_owned()
        } else {
            let name = country_name(rules, opponent.country);
            let (display_key, _) = rules.country_display_name_sources(name);
            display_key
                .map(|key| localized(state, key, name))
                .unwrap_or_else(|| name.to_owned())
        };
        let (key, fallback) = opponent.row_type.label();
        rows.push(AiCommanderRow {
            slot_index: index,
            label: format!("Computer {}", index + 1),
            country,
            difficulty: localized(state, key, fallback),
            choices,
            selected: opponent.commander.clone(),
        });
    }
    let notice = state.frontend.ai_commanders_notice.clone();
    state.frontend.ai_commanders_setup = Some(AiCommanderSetupState::new(
        rows,
        state.persistence.options_profile.commander_feedback,
        notice,
    ));
    state.frontend.skirmish_shell_state.open_combo_dropdown = None;
    state.frontend.ai_commanders_buttons = Default::default();
    crate::ui::skirmish_shell::blur_player_name_edit(&mut state.frontend.skirmish_shell_state);
    state.platform.window.request_redraw();
}

pub(crate) fn apply_effect(state: &mut AppState, effect: AiCommanderSetupEffect) {
    match effect {
        AiCommanderSetupEffect::None => return,
        AiCommanderSetupEffect::Cancel => state.frontend.ai_commanders_setup = None,
        AiCommanderSetupEffect::Apply {
            choices,
            feedback_enabled,
        } => {
            let Some(setup) = state.frontend.ai_commanders_setup.take() else {
                return;
            };
            for (index, name) in choices {
                if setup.rows.iter().any(|row| {
                    row.slot_index == index && row.choices.iter().any(|choice| choice.name == name)
                }) && let Some(opponent) =
                    state.frontend.skirmish_shell_state.opponents.get_mut(index)
                {
                    opponent.commander = name;
                }
            }
            state.persistence.options_profile.commander_feedback = feedback_enabled;
            state.frontend.ai_commanders_notice = None;
            reconcile(state);
            state.frontend.multiplayer_shell_runtime.pack_durable_state(
                &state.frontend.skirmish_shell_state,
                state.frontend.scenario_catalog.shell_maps(),
                &state.frontend.skirmish_modes,
            );
            state.frontend.multiplayer_shell_runtime.persist_snapshot();
            crate::app::persistence::options::persist_options_profile(state);
        }
    }
    state.frontend.ai_commanders_buttons = Default::default();
    state.platform.window.request_redraw();
}

pub(crate) fn resolve_launch(rules: &RuleSet, session: &mut SkirmishLaunchSession) {
    for slot in &mut session.opponents {
        let Some(requested) = slot.commander.as_deref() else {
            continue;
        };
        let country = country_name(rules, slot.country);
        let key = (u64::from(slot.raw_slot_index) << 40)
            | (u64::from(slot.color_index) << 32)
            | u64::from(slot.country.raw_index());
        let catalog = rules.personalities();
        let resolved = catalog
            .resolve_for_country(country, requested, key)
            .and_then(|id| catalog.get(id));
        if let Some(definition) = resolved {
            slot.commander = Some(definition.name.clone());
        } else {
            slot.commander = Some(PersonalityCatalog::STANDARD.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::ini_parser::IniFile;
    use crate::skirmish_launch::LaunchCountry;

    #[test]
    fn setup_retains_compatible_names_and_visibly_resets_missing_or_changed_countries() {
        let rules = RuleSet::from_ini(&IniFile::from_str("[Countries]\n0=Americans\n1=Russians\n[AIPersonalities]\n0=Warren\n[Warren]\nCountry=Americans\n")).unwrap();
        let mut shell = SkirmishShellState::default();
        shell.opponents[0].country = LaunchCountry::from_country_index(0);
        shell.opponents[0].country_random = false;
        shell.opponents[0].commander = "warren".into();
        assert!(reconcile_choices(&mut shell, &rules).is_empty());
        assert_eq!(shell.opponents[0].commander, "Warren");
        shell.opponents[0].country = LaunchCountry::from_country_index(1);
        shell.opponents[28].commander = "Missing".into();
        assert_eq!(reconcile_choices(&mut shell, &rules), [1, 29]);
        assert_eq!(shell.opponents[0].commander, "Standard");
        assert_eq!(shell.opponents[28].commander, "Standard");
        shell.opponents[0].country_random = true;
        shell.opponents[0].commander = "Random".into();
        assert!(reconcile_choices(&mut shell, &rules).is_empty());
        assert_eq!(shell.opponents[0].commander, "Random");
    }
}
