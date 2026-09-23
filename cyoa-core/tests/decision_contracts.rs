//! Project expectations from docs/decisions override Python parity.
//! Changes to these expectations require an explicit decision, not a source refresh.

use cyoa_core::{
    character::{
        Character, CharacterCast, CharacterDelta, CharacterDetails, CharacterDetailsFields,
    },
    ids::CharacterId,
    summary::EventList,
    text::{CharacterDescription, CharacterName, CharacterSituation, QuickActionText},
    turn::{QuickAction, QuickActionKind, QuickActions},
};

#[test]
fn text_002_unicode_lowercase_is_not_python_casefold() {
    let events = EventList::new(["Straße", "STRASSE", "STRAẞE"]);
    assert_eq!(
        events
            .iter()
            .map(|event| event.as_str())
            .collect::<Vec<_>>(),
        ["Straße", "STRASSE"]
    );
    let actions =
        QuickActions::select(["Straße", "STRASSE", "STRAẞE"].map(|text| {
            QuickAction::new(QuickActionText::new(text).unwrap(), QuickActionKind::Other)
        }))
        .unwrap();
    assert_eq!(
        actions
            .as_slice()
            .iter()
            .map(|a| a.text().as_str())
            .collect::<Vec<_>>(),
        ["Straße", "STRASSE"]
    );
    assert_ne!(
        CharacterId::for_name("Straße"),
        CharacterId::for_name("STRASSE")
    );
    assert_eq!(
        CharacterId::for_name("Straße"),
        CharacterId::for_name("STRAẞE")
    );

    let cast = CharacterCast::new([("one", "Straße"), ("two", "STRASSE")].map(|(id, name)| {
        Character::new(
            CharacterId::new(id).unwrap(),
            CharacterName::new(name).unwrap(),
            CharacterDetails::new(CharacterDetailsFields {
                description: Some(CharacterDescription::new("A character").unwrap()),
                ..Default::default()
            })
            .unwrap(),
        )
    }))
    .unwrap();
    let updated = cast.updated(&[CharacterDelta {
        name: Some(CharacterName::new("STRAẞE").unwrap()),
        current_state: Some(CharacterSituation::new("at the gate").unwrap()),
        ..Default::default()
    }]);
    assert_eq!(
        updated
            .get(&CharacterId::new("one").unwrap())
            .unwrap()
            .details()
            .current_state()
            .unwrap()
            .as_str(),
        "at the gate"
    );
    assert!(
        updated
            .get(&CharacterId::new("two").unwrap())
            .unwrap()
            .details()
            .current_state()
            .is_none()
    );
}
