#[test]
fn catalog_matches_paper_shaders() {
    let mut ours: Vec<&str> =
        super::TRANSITIONS.iter().map(|spec| spec.key).filter(|key| *key != "random").collect();
    ours.sort_unstable();
    assert!(paper_shaders::EFFECTS.iter().any(|(key, _)| *key == paper_shaders::DRAWING_SHADER));
    let mut shaders: Vec<&str> = paper_shaders::EFFECTS
        .iter()
        .map(|(key, _)| *key)
        .filter(|key| *key != paper_shaders::DRAWING_SHADER)
        .chain(paper_shaders::SAND_STYLES.iter().copied())
        .collect();
    shaders.sort_unstable();
    assert_eq!(ours, shaders);
}

#[test]
fn studio_presets_match_paper() {
    let studio: Vec<&str> = super::keys_in_family(super::Family::Studio).collect();
    assert_eq!(studio.len(), super::STUDIO_TRANSITIONS.len());
    for preset in super::STUDIO_TRANSITIONS {
        assert!(studio.contains(&preset.id));
        assert_eq!(super::preset_duration_ms(preset.id), Some(preset.duration_ms));
    }
    assert_eq!(super::preset_duration_ms("random"), None);
    assert_eq!(super::preset_duration_ms("crossfade"), None);
}
