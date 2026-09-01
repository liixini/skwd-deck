pub fn matches_tag_spec(tokens: &[String], spec: &str) -> bool {
    let mut any_positive = false;
    for term in spec.split(',').map(str::trim).filter(|term| !term.is_empty()) {
        if let Some(neg) = term.strip_prefix('-') {
            let neg = neg.trim().to_lowercase();
            if !neg.is_empty() && tokens.contains(&neg) {
                return false;
            }
        } else {
            any_positive = true;
            let matches = term
                .split('|')
                .map(|alt| alt.trim().to_lowercase())
                .filter(|alt| !alt.is_empty())
                .any(|alt| tokens.contains(&alt));
            if !matches {
                return false;
            }
        }
    }
    any_positive
}
