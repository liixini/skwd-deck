use super::*;
use crate::studio::{StudioPalette, Tuning, tune};

#[test]
fn keys_round_trip() {
    for key in ["tonal", "theme:nord", "material:scheme-content"] {
        assert_eq!(Source::parse(key).key(), key);
    }
    assert_eq!(Source::parse("theme:nord"), Source::Theme("nord".into()));
    assert_eq!(Source::parse("material:scheme-rainbow"), Source::Material("scheme-rainbow".into()));
    assert_eq!(Source::parse("subject"), Source::Method("subject".into()));
}

struct Fixed(StudioPalette, StudioPalette);

impl Reading for Fixed {
    fn material(&self, scheme: &str, dark: bool) -> Option<StudioPalette> {
        (scheme == "content").then_some(if dark { self.0 } else { self.1 })
    }

    fn method(&self, key: &str, dark: bool) -> Option<StudioPalette> {
        (key == "tonal").then_some(if dark { self.0 } else { self.1 })
    }
}

fn fixed() -> Fixed {
    let dark = crate::studio::from_theme(&crate::preset("nord").expect("nord"));
    let light = crate::studio::from_theme(&crate::preset("graphite-light").expect("graphite"));
    Fixed(dark, light)
}

#[test]
fn methods_follow_mode() {
    let reading = fixed();
    let tuning = Tuning::default();
    let (dark, is_dark) =
        resolve(&Source::parse("tonal"), Some(&reading), &tuning, || true).expect("dark");
    assert!(is_dark);
    assert_eq!(dark.base, tune(&reading.0, &tuning, true).base);
    let (_, is_dark) =
        resolve(&Source::parse("tonal"), Some(&reading), &tuning, || false).expect("light");
    assert!(!is_dark);
    assert!(resolve(&Source::parse("pigment"), Some(&reading), &tuning, || true).is_none());
}

#[test]
fn themes_ignore_mode() {
    let tuning = Tuning::default();
    let (_, is_dark) =
        resolve(&Source::parse("theme:graphite-light"), None::<&Fixed>, &tuning, || {
            panic!("mode read")
        })
        .expect("theme");
    assert!(!is_dark);
}

#[test]
fn material_needs_reading() {
    let tuning = Tuning::default();
    let source = Source::parse("material:content");
    assert!(resolve(&source, None::<&Fixed>, &tuning, || true).is_none());
    assert!(resolve(&source, Some(&fixed()), &tuning, || true).is_some());
    assert!(resolve(&Source::parse("material:smart"), Some(&fixed()), &tuning, || true).is_none());
}

#[test]
fn metals_follow_mode() {
    let tuning = Tuning::default();
    let copper = Source::parse("theme:copper");
    let (dark, is_dark) = resolve(&copper, None::<&Fixed>, &tuning, || true).expect("dark");
    let (light, is_light_dark) =
        resolve(&copper, None::<&Fixed>, &tuning, || false).expect("light");
    assert!(is_dark);
    assert!(!is_light_dark);
    let twin = crate::studio::from_theme(&crate::preset("copper-light").expect("twin"));
    assert_eq!(light.base, tune(&twin, &tuning, false).base);
    assert_ne!(dark.base, light.base);
}
