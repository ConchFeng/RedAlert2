//! Original FirstBuildable5051E0 mixed identity gates, not a second selector.
use super::*;
use crate::rules::ini_parser::IniFile;

fn mask_names(mask: i64) -> String {
    if mask == -1 {
        return String::new();
    }
    (0..32)
        .filter(|bit| mask & (1i64 << bit) != 0)
        .map(|bit| match bit {
            0 => "BASE".into(),
            1 => "PARENT".into(),
            2 => "CHILD".into(),
            _ => format!("N{bit}"),
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn first_buildable_keeps_parent_owner_and_actual_restrictions_separate() {
    let gold = crate::test_fixture::json("tools/rules_oracle/session_country_parent.json");
    for row in gold["first_buildable_controls"].as_array().unwrap() {
        let mut text = String::from("[Countries]\n0=BASE\n1=PARENT\n2=CHILD\n");
        for index in 3..32 {
            text.push_str(&format!("{index}=N{index}\n"));
        }
        let owner = row["owner_text"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| mask_names(row["owner_mask"].as_i64().unwrap()));
        text.push_str(&format!("[PARENT]\nName=ParentAlias\n[CHILD]\nParentCountry={}\n[BuildingTypes]\n0=TARGET\n[TARGET]\nStrength=100\nAIBasePlanningSide=0\nOwner={}\n",
            row["parent"].as_str().unwrap(), owner));
        for (field, key) in [
            ("required_mask", "RequiredHouses"),
            ("forbidden_mask", "ForbiddenHouses"),
        ] {
            if row[field].as_i64().unwrap() != -1 {
                text.push_str(&format!(
                    "{key}={}\n",
                    mask_names(row[field].as_i64().unwrap())
                ));
            }
        }
        let rules = RuleSet::from_ini(&IniFile::from_str(&text)).unwrap();
        let selected =
            first_buildable_from_array(&rules, &["TARGET".into()], "CHILD", 0, true).is_some();
        assert_eq!(
            selected,
            row["selected"].as_bool().unwrap(),
            "{}",
            row["id"]
        );
        assert_eq!(
            house_country_bit(&rules, "CHILD"),
            4,
            "actual restrictions do not inherit"
        );
    }
}

#[test]
fn opening_mcv_uses_parent_owner_without_production_restrictions() {
    let rules = RuleSet::from_ini(&IniFile::from_str(
        "[Countries]\n0=ZERO\n1=PARENT\n2=CHILD\n[CHILD]\nParentCountry=PARENT\n\
         [General]\nBaseUnit=WRONG,RIGHT\n[VehicleTypes]\n0=WRONG\n1=RIGHT\n\
         [WRONG]\nStrength=100\nOwner=CHILD\n\
         [RIGHT]\nStrength=100\nOwner=PARENT\nRequiredHouses=ZERO\nForbiddenHouses=CHILD\n",
    ))
    .unwrap();
    assert_eq!(
        crate::sim::scenario_bootstrap::launch_mcv_type_for_country(
            crate::skirmish_launch::LaunchCountry::from_country_index(2),
            &rules
        ),
        Some("RIGHT")
    );
}

/// The load-time SHP preload must take exactly the types whose body is a
/// sprite: infantry, building hulls, and a vehicle or aircraft whose ART row
/// turns `Voxel=` off. Voxel vehicles would only duplicate the unit atlas.
#[test]
fn producible_shp_types_follow_the_art_body_not_only_the_category() {
    let mut rules = RuleSet::from_ini(&IniFile::from_str(
        "[Countries]\n0=AMER\n[AMER]\nParentCountry=AMER\n\
         [InfantryTypes]\n0=E1\n[VehicleTypes]\n0=GRIZ\n1=SHPVAN\n\
         [AircraftTypes]\n0=ORCA\n1=SHPJET\n[BuildingTypes]\n0=GAPOWR\n\
         [E1]\nOwner=AMER\n[GRIZ]\nOwner=AMER\n[SHPVAN]\nOwner=AMER\n\
         [ORCA]\nOwner=AMER\n[SHPJET]\nOwner=AMER\n[GAPOWR]\nOwner=AMER\n",
    ))
    .unwrap();
    rules.install_art_data(crate::rules::art_data::ArtRegistry::from_ini(
        &IniFile::from_str("[SHPVAN]\nVoxel=no\n[SHPJET]\nVoxel=no\n"),
    ));
    let masks = CountryBuildMasks::for_country(&rules, "AMER");
    let types = producible_shp_types_for_country(masks, 0, &rules, false);
    assert!(types.contains(&"E1"), "infantry bodies are SHP");
    assert!(types.contains(&"GAPOWR"), "building hulls are SHP");
    assert!(types.contains(&"SHPVAN"), "Voxel=no keeps a vehicle on SHP");
    assert!(types.contains(&"SHPJET"), "Voxel=no keeps an aircraft on SHP");
    assert!(
        !types.contains(&"GRIZ"),
        "a default vehicle belongs to the voxel atlas"
    );
    assert!(
        !types.contains(&"ORCA"),
        "a default aircraft belongs to the voxel atlas"
    );
}
