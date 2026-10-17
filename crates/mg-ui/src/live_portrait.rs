//! Portrait defaults shared by Selection, Focus and Chat.
use mg_2da::TwoDa;

pub(crate) fn gender(o: &mg_live::store::LiveObject) -> Option<u8> {
    o.look.as_ref()?.get("Gender")?.get("value")?.as_u64()?.try_into().ok()
}

pub(crate) fn default_base(
    game: &mg_rules::GameData,
    appearance: Option<u16>,
    gender: Option<u8>,
) -> Option<String> {
    let row = usize::from(appearance?);
    let appearance = game.table("appearance").ok()?;
    if let Some(base) = appearance.get(row, "PORTRAIT") {
        return Some(base.to_owned());
    }
    // The stock dynamic humanoid rows (including Human = 6) have no PORTRAIT.
    // Resolve their race through racialtypes.Appearance, NOT appearance.RACIALTYPE:
    // the latter is an engine category (Human has 11), not a portraits.Race row.
    if appearance.get(row, "MODELTYPE")? != "P" {
        return None;
    }
    humanoid_base(
        game.table("racialtypes").ok()?.as_ref(),
        game.table("portraits").ok()?.as_ref(),
        row,
        gender,
    )
}

fn humanoid_base(
    races: &TwoDa,
    portraits: &TwoDa,
    appearance: usize,
    gender: Option<u8>,
) -> Option<String> {
    let race =
        (0..races.len()).find(|&r| races.get_int(r, "Appearance") == Some(appearance as i32))?;
    // Old/minimal reports omit Gender. A stable male default matches the
    // creature editor's initial gender; an explicit female look uses her row.
    let sex = i32::from(gender.unwrap_or(0));
    (0..portraits.len()).find_map(|r| {
        (portraits.get_int(r, "Race") == Some(race as i32)
            && portraits.get_int(r, "Sex") == Some(sex)
            && portraits.get_int(r, "Plot").unwrap_or(0) == 0
            && portraits.get_int(r, "InanimateType").is_none())
        .then(|| portraits.get(r, "BaseResRef"))
        .flatten()
        .map(|base| format!("po_{base}"))
    })
}

pub(crate) fn load<T>(
    base: Option<&str>,
    fallback: Option<&str>,
    mut picture: impl FnMut(&str) -> Option<T>,
) -> Option<T> {
    [base, fallback].into_iter().flatten().find_map(|base| {
        let base = base.trim_matches(|c: char| c.is_whitespace() || c == '\0');
        if base.is_empty() || base.eq_ignore_ascii_case("po_") || base == "****" {
            return None;
        }
        [base.to_string(), format!("{}_", base.trim_end_matches('_'))].iter().find_map(|base| {
            ['s', 'm', 't', 'l', 'h'].into_iter().find_map(|size| picture(&format!("{base}{size}")))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(text: &str) -> TwoDa {
        TwoDa::parse(format!("2DA V2.0\n\n{text}").as_bytes(), mg_core::Codepage::WINDOWS_1252)
            .unwrap()
    }

    #[test]
    fn live_portrait_appearance_uses_the_reported_row() {
        let mut resources = mg_resman::ResMan::new();
        let mut memory = mg_resman::MemContainer::new();
        memory.insert(
            mg_resman::ResKey::parse("appearance", mg_core::ResType::TWODA).unwrap(),
            b"2DA V2.0\n\nPORTRAIT MODELTYPE\n0 po_bird S\n1 po_hound S\n2 **** S\n".to_vec(),
        );
        resources.add(100, "portrait-test", mg_resman::LayerClass::Directory, memory);
        let game = mg_rules::GameData::new(resources, mg_tlk::Tlk::new(mg_core::Language::ENGLISH));
        assert_eq!(default_base(&game, Some(1), None).as_deref(), Some("po_hound"));
        assert_eq!(default_base(&game, Some(0), None).as_deref(), Some("po_bird"));
        assert_eq!(default_base(&game, Some(2), None), None);
        assert_eq!(default_base(&game, Some(99), None), None);
        assert_eq!(default_base(&game, None, None), None);
    }

    #[test]
    fn live_portrait_humanoid_uses_race_mapping_and_gender_not_appearance_row() {
        let races = table("Appearance\n0 42\n1 6\n");
        let portraits = table(
            "BaseResRef Sex Race Plot InanimateType\n0 wrong_ 0 6 0 ****\n1 plot_ 0 1 1 ****\n2 door_ 0 1 0 0\n3 human_m_ 0 1 0 ****\n4 human_f_ 1 1 0 ****\n5 other_ 0 0 0 ****\n",
        );
        assert_eq!(humanoid_base(&races, &portraits, 6, Some(0)).as_deref(), Some("po_human_m_"));
        assert_eq!(humanoid_base(&races, &portraits, 6, Some(1)).as_deref(), Some("po_human_f_"));
        assert_eq!(humanoid_base(&races, &portraits, 42, None).as_deref(), Some("po_other_"));
        assert_eq!(humanoid_base(&races, &portraits, 999, None), None);
    }

    #[test]
    fn live_portrait_custom_sizes_and_appearance_defaults_precede_missing_placeholder() {
        let available = |s: &str| {
            ["custom_h", "po_bird_s", "po_human_s", "po_unknown_s"]
                .contains(&s)
                .then(|| s.to_owned())
        };
        assert_eq!(load(Some("custom_"), Some("po_bird"), available).as_deref(), Some("custom_h"));
        assert_eq!(load(Some("po_"), Some("po_bird"), available).as_deref(), Some("po_bird_s"));
        assert_eq!(
            load(Some("missing"), Some("po_human_"), available).as_deref(),
            Some("po_human_s")
        );
        assert_eq!(load(Some(" \0po_\0 "), None, available), None);
        assert_eq!(load(Some("****"), None, available), None);
    }
}
