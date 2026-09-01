#[test]
fn catalog_matches_paper_shaders() {
    let mut ours: Vec<&str> =
        super::TRANSITIONS.iter().map(|spec| spec.key).filter(|key| *key != "random").collect();
    ours.sort_unstable();
    let mut shaders: Vec<&str> = paper_shaders::EFFECTS
        .iter()
        .map(|(key, _)| *key)
        .chain(paper_shaders::SAND_STYLES.iter().copied())
        .collect();
    shaders.sort_unstable();
    assert_eq!(ours, shaders);
}
