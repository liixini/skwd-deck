use super::*;

#[test]
fn generated_templates_require_complete_consistent_colours() {
    let doc = crate::material::document("#226688", true).unwrap();
    for name in ["kitty.conf", "yazi-theme.toml"] {
        let source = template(name).unwrap();
        let rendered = super::super::render_doc(source, &doc);
        assert!(generated(name, &rendered));
        assert!(!generated(name, &format!("{rendered}\n# user edit")));
        assert!(!generated(name, &rendered.replacen("#ffb4ab", "#123456", 1)));
        assert!(!generated(name, &rendered.replacen('#', "é", 1)));
        assert!(!generated(name, source));
    }
}

#[test]
fn ordinary_terminal_colours_follow_palette_but_errors_remain_errors() {
    let kitty = include_str!("../../../../../data/matugen/templates/kitty.conf");
    let yazi = include_str!("../../../../../data/matugen/templates/yazi-theme.toml");
    let mut previous = None;
    for seed in ["#2244aa", "#22aa44"] {
        for dark in [true, false] {
            let doc = crate::material::document(seed, dark).unwrap();
            let role = |key| crate::material::role(&doc, key, "default").unwrap();
            let text = super::super::render_doc(kitty, &doc);
            for slot in ["color1", "color9"] {
                let value = text
                    .lines()
                    .find_map(|line| {
                        let mut fields = line.split_whitespace();
                        (fields.next() == Some(slot)).then(|| fields.next().unwrap())
                    })
                    .unwrap();
                assert_eq!(value, role("secondary"));
                assert_ne!(value, role("error"));
                assert_ne!(previous.as_deref(), Some(value));
            }
            previous = Some(role("secondary"));
            let text = super::super::render_doc(yazi, &doc);
            assert!(text.contains(&format!("perm_exec  = {{ fg = \"{}\" }}", role("primary"))));
            assert!(text.contains(&format!(
                "progress_error  = {{ fg = \"{}\", bg = \"reset\" }}",
                role("error")
            )));
        }
    }
}
