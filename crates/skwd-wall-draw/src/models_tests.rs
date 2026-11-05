use super::*;

#[test]
fn depth_input_multiples() {
    assert_eq!(depth_input(2560, 1440), [924, 518]);
    assert_eq!(depth_input(1440, 2560), [518, 924]);
    assert_eq!(depth_input(100, 50), [518, 266]);
}

#[test]
fn longest_side_input() {
    assert_eq!(longest_input(2560, 1440, DEPTH3_SIDE), [504, 280]);
    assert_eq!(longest_input(1440, 2560, DEPTH3_SIDE), [280, 504]);
    assert_eq!(longest_input(1600, 900, 1008.0), [1008, 574]);
}

#[test]
fn spread_orders_far() {
    let plane = Plane { width: 4, height: 1, data: vec![-3.0, 1.0, 5.0, 9.0] };
    let spread = spread(plane);
    assert!(spread.data.windows(2).all(|pair| pair[0] <= pair[1]));
    assert_eq!(spread.data[0], 0.0);
    assert_eq!(spread.data[3], 1.0);
}

#[test]
fn negated_spread_mirrors() {
    let near = vec![0.4, 7.0, 2.5, 11.0, 3.0, 0.0, 9.5, 6.0, 1.2, 4.4];
    let plain = spread(Plane { width: near.len(), height: 1, data: near.clone() });
    let negated = spread(Plane {
        width: near.len(),
        height: 1,
        data: near.iter().map(|level| -level).collect(),
    });
    for (mirror, level) in negated.data.iter().zip(plain.data) {
        assert!((mirror - (1.0 - level)).abs() < 1e-6);
    }
}

#[test]
fn sky_goes_far() {
    let distance = [1.0, 2.0, 3.0, 50.0];
    let sky = [0.0, 0.0, 0.0, 0.9];
    let farther = far_from_distance(&distance, &sky, [4, 1], [4, 1]).unwrap();
    assert_eq!(farther.data[3], 1.0);
    assert!(farther.data[0] < farther.data[2]);
}

#[test]
fn rejects_sky_shape() {
    assert!(far_from_distance(&[1.0, 2.0], &[0.0], [2, 1], [2, 1]).is_err());
}

#[test]
fn all_sky_is_far() {
    let farther = far_from_distance(&[5.0, 6.0], &[1.0, 1.0], [2, 1], [2, 1]).unwrap();
    assert!(farther.data.iter().all(|&level| level == 1.0));
}

#[test]
fn decodes_centre_box() {
    let bounds = decode_box(&[0.5, 0.5, 0.25, 0.5], [100, 50]).unwrap();
    assert_eq!(bounds, Bounds { left: 37.5, top: 12.5, right: 62.5, bottom: 37.5 });
}

#[test]
fn clamps_edge_box() {
    let bounds = decode_box(&[0.0, 0.0, 0.5, 0.5], [100, 100]).unwrap();
    assert_eq!(bounds, Bounds { left: 0.0, top: 0.0, right: 25.0, bottom: 25.0 });
}

#[test]
fn rejects_inverted_box() {
    assert!(decode_box(&[0.5, 0.5, -0.2, 0.2], [100, 100]).is_none());
    assert!(decode_box(&[1.5, 0.5, 0.2, 0.2], [100, 100]).is_none());
}

#[test]
fn planar_pads_edges() {
    let mut image = RgbImage::new(2, 1);
    image.put_pixel(1, 0, image::Rgb([255, 0, 0]));
    let pixels = planar(&image, [4, 2], |_, value| value);
    assert_eq!(&pixels[..4], &[0.0, 1.0, 1.0, 1.0]);
    assert_eq!(&pixels[4..8], &[0.0, 1.0, 1.0, 1.0]);
    assert!(pixels[8..].iter().all(|value| *value == 0.0));
}

#[test]
fn letterbox_centres_image() {
    let framed = letterbox(&RgbImage::new(20, 10), 8, 7);
    assert_eq!((framed.width, framed.height, framed.left, framed.top), (8, 4, 0, 2));
    assert_eq!(framed.scale, 0.4);
    assert_eq!(framed.image.get_pixel(0, 0)[0], 7);
    assert_eq!(framed.image.get_pixel(0, 2)[0], 0);
}

#[test]
fn reads_both_layouts() {
    let mut anchors = [[1.0, 1.0, 1.0, 1.0, 0.1]; 6];
    anchors[2] = [10.0, 20.0, 4.0, 6.0, 0.9];
    let by_anchor: Vec<f32> = anchors.iter().flatten().copied().collect();
    let by_feature: Vec<f32> =
        (0..5).flat_map(|feature| anchors.map(|anchor| anchor[feature])).collect();
    let expected = vec![(0.9, [10.0, 20.0, 4.0, 6.0])];
    assert_eq!(candidates(&by_anchor, [6, 5]), expected);
    assert_eq!(candidates(&by_feature, [5, 6]), expected);
}

#[test]
fn suppresses_overlaps() {
    let found = vec![
        (0.9, [10.0, 10.0, 10.0, 10.0]),
        (0.8, [11.0, 10.0, 10.0, 10.0]),
        (0.7, [40.0, 40.0, 10.0, 10.0]),
    ];
    let kept = suppress(found, |[cx, cy, width, height]| Bounds {
        left: cx - width / 2.0,
        top: cy - height / 2.0,
        right: cx + width / 2.0,
        bottom: cy + height / 2.0,
    });
    assert_eq!(kept.len(), 2);
    assert_eq!(kept[1].middle(), [40.0, 40.0]);
}

#[test]
fn overlap_is_iou() {
    let unit = Bounds { left: 0.0, top: 0.0, right: 2.0, bottom: 2.0 };
    let shifted = Bounds { left: 1.0, top: 0.0, right: 3.0, bottom: 2.0 };
    assert!((unit.overlap(&shifted) - 1.0 / 3.0).abs() < 1e-6);
}

#[test]
fn unframes_to_source() {
    let framed = letterbox(&RgbImage::new(200, 100), 100, 0);
    let bounds = framed.unframe([50.0, 50.0, 20.0, 10.0], 2.0, [400, 200]);
    assert_eq!(bounds, Bounds { left: 160.0, top: 80.0, right: 240.0, bottom: 120.0 });
}

#[test]
fn device_order_table() {
    let cases = [
        (Device::Cpu, true, &[Backend::Cpu][..]),
        (Device::Cuda, true, &[Backend::Cuda]),
        (Device::Webgpu, true, &[Backend::Webgpu]),
        (Device::Auto, true, &[Backend::Cuda, Backend::Webgpu, Backend::Cpu]),
        (Device::Auto, false, &[Backend::Cuda, Backend::Cpu]),
    ];
    for (device, webgpu, expected) in cases {
        assert_eq!(order(device, webgpu), expected, "{device:?}");
    }
}

#[test]
fn falls_back_once() {
    let mut tried = Vec::new();
    let order = order(Device::Auto, true);
    let (backend, value) = first_working(order, |backend| {
        tried.push(backend);
        ensure!(backend == Backend::Webgpu, "no {backend:?}");
        Ok(7)
    })
    .unwrap();
    assert_eq!((backend, value), (Backend::Webgpu, 7));
    assert_eq!(tried, [Backend::Cuda, Backend::Webgpu]);
}

#[test]
fn explicit_device_fails() {
    let order = order(Device::Cuda, true);
    let error = first_working(order, |_| anyhow::bail!("no cuda") as Result<()>).unwrap_err();
    assert_eq!(error.to_string(), "no cuda");
}

#[test]
fn keeps_input_errors() {
    let input = anyhow::anyhow!("wallpaper source hash changed").context("prepare");
    assert!(!from_runtime(&input));
    assert!(!falls_back(Device::Auto, Backend::Webgpu, &input));
}

#[test]
fn webgpu_options_prefixed() {
    let plain = webgpu_options(&[], None);
    assert_eq!(plain[0], ("WebGpuExecutionProvider.preferredLayout".into(), "NCHW".into()));
    assert_eq!(
        plain[2],
        ("WebGpuExecutionProvider.powerPreference".into(), "high-performance".into())
    );
    assert_eq!(plain.len(), 3);
    let nodes = webgpu_options(&["/head".into(), "/tail".into()], None);
    let last = nodes.last().unwrap();
    assert_eq!(last, &("WebGpuExecutionProvider.forceCpuNodeNames".into(), "/head\n/tail".into()));
}

const CHOSEN: Uuid = [7; skwd_config::GPU_UUID_BYTES];

#[test]
fn places_found_gpu() {
    for (device, backend) in [(Device::Auto, Backend::Cuda), (Device::Webgpu, Backend::Webgpu)] {
        let placed = place(device, backend, Some(&CHOSEN), |_| Ok(Some(1))).unwrap();
        assert_eq!(placed, Some(1), "{device:?} {backend:?}");
    }
}

#[test]
fn auto_cuda_needs_match() {
    assert!(place(Device::Auto, Backend::Cuda, Some(&CHOSEN), |_| Ok(None)).is_err());
    assert!(place(Device::Auto, Backend::Cuda, Some(&CHOSEN), |_| bail!("no driver")).is_err());
}

#[test]
fn explicit_falls_back() {
    for (device, backend) in [(Device::Cuda, Backend::Cuda), (Device::Webgpu, Backend::Webgpu)] {
        let missing = place(device, backend, Some(&CHOSEN), |_| Ok(None)).unwrap();
        let failed = place(device, backend, Some(&CHOSEN), |_| bail!("no driver")).unwrap();
        assert_eq!((missing, failed), (None, None), "{device:?} {backend:?}");
    }
}

#[test]
fn auto_webgpu_falls_back() {
    let placed = place(Device::Auto, Backend::Webgpu, Some(&CHOSEN), |_| Ok(None)).unwrap();
    assert_eq!(placed, None);
}

#[test]
fn auto_gpu_untouched() {
    let placed = place(Device::Auto, Backend::Cuda, None, |_| panic!("searched")).unwrap();
    assert_eq!(placed, None);
}

#[test]
fn webgpu_adapter_options() {
    let options = webgpu_options(&[], Some(1));
    assert_eq!(
        options[options.len() - 2..],
        [
            ("WebGpuExecutionProvider.adapterIndex".into(), "1".into()),
            ("WebGpuExecutionProvider.dawnBackendType".into(), "Vulkan".into()),
        ]
    );
}
