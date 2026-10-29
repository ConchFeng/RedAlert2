//! ReadPip's frame/index trap through its production reader and type-pass owner.
use super::{InfantryPips, ObjectCategory};
use crate::rules::ini_parser::{IniFile, IniSection};
use crate::rules::native_processing::{NativeRulesRegistryState, RulesLayerStack};
use crate::rules::ruleset::RuleSet;

fn native() -> &'static serde_json::Value {
    crate::test_fixture::json("tools/procedural_drawing_oracle/passenger_pips.json")
}

fn cached_ini(sections: &serde_json::Value) -> IniFile {
    let mut ini = IniFile::empty();
    for (name, keys) in sections.as_object().unwrap() {
        let mut section = IniSection::new(name.clone());
        for (key, value) in keys.as_object().unwrap() {
            section.set(key, value.as_str().unwrap());
        }
        ini.replace_first_section(section);
    }
    ini
}

#[test]
fn original_read_pip_controls_use_string_defaults_and_mapped_frames() {
    let native = native();
    assert_eq!(native["original_code_unchanged"], true);
    let npbug = crate::rules::NPBUG_ENABLED;
    let controls = native["read_pip"]["controls"].as_array().unwrap();
    assert!(!controls.is_empty());
    let mut adjacent_defaults = Vec::new();
    for row in controls {
        let input = &row["input"];
        let ini = cached_ini(&input["sections"]);
        let section = ini.section(input["section"].as_str().unwrap()).unwrap();
        let current = input["default"].as_u64().unwrap() as u32;
        // NPBUG keeps the current frame when no pass holds the key (issue
        // #2929); a held raw value still scans to the golden result.
        let expected = if npbug && input["raw"].is_null() {
            current
        } else {
            row["result"].as_u64().unwrap() as u32
        };
        assert_eq!(
            section.read_pip(input["key"].as_str().unwrap(), current),
            expected,
            "{row}"
        );
        if input["raw"].is_null() && matches!(current, 11 | 12) {
            if !npbug {
                assert_eq!(row["result"], 1, "adjacent names are not pip matches");
            }
            adjacent_defaults.push(current);
        }
    }
    assert!(adjacent_defaults.contains(&11) && adjacent_defaults.contains(&12));
    for row in native["read_pip"]["table"].as_array().unwrap() {
        let mut section = IniSection::new("PipProbe".to_owned());
        section.set("Pip", row["name"].as_str().unwrap());
        let expected = if row["in_pip_table"] == true {
            row["value"].as_u64().unwrap() as u32
        } else {
            1
        };
        assert_eq!(section.read_pip("Pip", 1), expected, "{row}");
    }
}

#[test]
fn original_infantry_pip_passes_survive_process_handoffs() {
    let corpus = &native()["read_pip"];
    let constructor = InfantryPips::default();
    assert_eq!(
        serde_json::to_value(constructor).unwrap(),
        corpus["constructor"]
    );
    let mut state = NativeRulesRegistryState::default();
    // NPBUG (issue #2929): keyless passes keep the last explicitly set
    // frames, so the expectation trails the last pass naming a pip key.
    let mut npbug_expected = corpus["constructor"].clone();
    for (index, row) in corpus["section_passes"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let mut pass = cached_ini(&row["input"]["sections"]);
        if index == 0 {
            let mut registry = IniSection::new("InfantryTypes".to_owned());
            registry.set("0", "E1");
            pass.replace_first_section(registry);
        }
        assert_eq!(
            pass.section("E1").is_some(),
            row["admitted"].as_bool().unwrap()
        );
        let processed = RulesLayerStack::new(pass)
            .process_with_fixed_art_and_registry_state(&IniFile::empty(), state)
            .unwrap();
        let rules = RuleSet::from_processed_rules(&processed).unwrap();
        let infantry = rules.object("E1").unwrap();
        // NPBUG (issue #2929): an admitted section that names neither pip key
        // keeps the retained frames instead of drifting them.
        let sets_pips = row["input"]["sections"]["E1"]
            .as_object()
            .is_some_and(|keys| keys.contains_key("Pip") || keys.contains_key("OccupyPip"));
        if sets_pips {
            npbug_expected = row["after"].clone();
        }
        let expected = if crate::rules::NPBUG_ENABLED {
            npbug_expected.clone()
        } else {
            row["after"].clone()
        };
        assert_eq!(
            serde_json::json!({"pip": infantry.pip(), "occupy_pip": infantry.occupy_pip()}),
            expected,
            "{}",
            row["name"]
        );
        let (_, trace) = processed.into_ini_and_native_type_construction_trace();
        state = trace.into_registry_state_discarding_events();
        // Saved process state retains the actual fields, not merged-key history.
        state = serde_json::from_value(serde_json::to_value(state).unwrap()).unwrap();
    }
}

#[test]
fn retail_pip_fields_and_capacities_match_original_readers() {
    let Some(retail) = crate::rules::retail_ini_fixture::retail_battle_rules_for_map("XMP03T4.MAP")
    else {
        return;
    };
    let native = native();
    let layers = native["readers"]["layers"].as_array().unwrap();
    let files: Vec<_> = layers
        .iter()
        .map(|row| row["file"].as_str().unwrap())
        .collect();
    assert_eq!(
        files,
        [
            "RULESMD.INI",
            "LANGRULE.INI",
            "MPBattleMD.ini",
            "XMP03T4.MAP"
        ]
    );
    assert_eq!(layers[1]["absent"], true);
    for (name, expected) in native["readers"]["type_results"].as_object().unwrap() {
        let object = retail.rules.object(name).unwrap();
        assert_eq!(
            object.pip_max(&retail.rules),
            expected["pip_max"].as_i64().unwrap() as i32,
            "{name}"
        );
        if object.category == ObjectCategory::Infantry {
            assert_eq!(
                object.pip(),
                expected["pip"].as_u64().unwrap() as u32,
                "{name}"
            );
            assert_eq!(
                object.occupy_pip(),
                expected["occupy_pip"].as_u64().unwrap() as u32,
                "{name}"
            );
        }
    }
}
