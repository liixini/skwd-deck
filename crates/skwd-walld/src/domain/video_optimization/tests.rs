use super::*;

#[test]
fn cover_height() {
    assert_eq!(fit_cap_height(3840, 2160, &[(2560, 1440), (1920, 1080)]), 1440);
    assert_eq!(fit_cap_height(3840, 2160, &[(1920, 1080)]), 1080);
    assert_eq!(fit_cap_height(3840, 2160, &[(3840, 2160), (1920, 1080)]), 0);
    assert_eq!(fit_cap_height(1920, 1080, &[(2560, 1440)]), 0);
    assert_eq!(fit_cap_height(2160, 3840, &[(2560, 1440)]), 0);
    assert_eq!(fit_cap_height(2160, 3840, &[(1440, 2560)]), 2560);
    assert_eq!(fit_cap_height(100, 101, &[(50, 50)]), 52);
}

#[test]
fn degenerate_geometry() {
    assert_eq!(fit_cap_height(0, 1080, &[(1920, 1080)]), 0);
    assert_eq!(fit_cap_height(1920, 0, &[(1920, 1080)]), 0);
    assert_eq!(fit_cap_height(3840, 2160, &[]), 0);
    assert_eq!(fit_cap_height(3840, 2160, &[(2560, 1440), (0, 0)]), 0);
}
