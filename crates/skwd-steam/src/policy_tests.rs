use super::*;

#[test]
fn numeric_ids_filtering() {
    let args: Vec<String> =
        ["4242", "", "not-an-id", "1069166728", "we:1", "12a3"].map(String::from).to_vec();
    assert_eq!(numeric_ids(&args), vec!["4242", "1069166728"]);
    assert!(numeric_ids(&[]).is_empty());
}

#[test]
fn install_and_download_state() {
    assert!(install_complete(true, false, false));
    assert!(!install_complete(true, true, false));
    assert!(!install_complete(true, false, true));
    assert!(!install_complete(false, false, false));

    assert!(download_active(true, false, 0));
    assert!(download_active(false, true, 0));
    assert!(download_active(false, false, 1));
    assert!(!download_active(false, false, 0));
}

#[test]
fn progress_is_bounded() {
    assert_eq!(progress_fraction(0, 0), 0.0);
    assert_eq!(progress_fraction(500, 0), 0.0);
    assert_eq!(progress_fraction(50, 100), 0.5);
    assert_eq!(progress_fraction(200, 100), 1.0);
}
