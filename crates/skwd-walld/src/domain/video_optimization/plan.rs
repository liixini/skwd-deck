pub(crate) fn fit_cap_height(src_w: u32, src_h: u32, outputs: &[(u32, u32)]) -> u32 {
    if src_w == 0 || src_h == 0 || outputs.is_empty() {
        return 0;
    }
    let mut needed_h = 0f64;
    for &(output_width, output_height) in outputs {
        if output_width == 0 || output_height == 0 {
            return 0;
        }
        let cover = (f64::from(output_width) / f64::from(src_w))
            .max(f64::from(output_height) / f64::from(src_h))
            .min(1.0);
        needed_h = needed_h.max(f64::from(src_h) * cover);
    }
    let capped = (needed_h.ceil() as u32 + 1) & !1;
    if capped >= src_h { 0 } else { capped }
}
