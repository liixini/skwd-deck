use super::*;

fn trace() -> Stroke {
    Stroke { points: vec![[1.0, 1.0], [6.0, 1.0]], width: 3.0, kind: Kind::Trace }
}

fn event(stroke: usize, start: f32, end: f32) -> Event {
    Event { stroke, start, end }
}

#[test]
fn writes_cache_files() {
    let directory = tempfile::tempdir().unwrap();
    let hatch = Stroke { points: vec![[0.0, 4.0], [8.0, -4.0]], width: 24.0, kind: Kind::Hatch };
    let strokes = [trace(), hatch];
    let farther = Plane::filled(8, 4, 0.25);
    let matte = Plane::filled(16, 4, 1.0);
    let events = [event(1, 0.5, 1.0), event(0, 0.0, 0.25)];
    let drawing = Drawing {
        strokes: &strokes,
        events: &events,
        farther: &farther,
        matte: &matte,
        ink: &[128; 64],
        ink_size: [16, 4],
    };
    drawing.write(directory.path()).unwrap();
    let planned = DrawingStroke::read(&directory.path().join(STROKES)).unwrap();
    assert_eq!(
        planned,
        [
            DrawingStroke {
                start: 0.5,
                end: 1.0,
                width: 24.0,
                hatch: true,
                points: vec![[0.0, 4.0], [8.0, -4.0]]
            },
            DrawingStroke {
                start: 0.0,
                end: 0.25,
                width: 3.0,
                hatch: false,
                points: vec![[1.0, 1.0], [6.0, 1.0]]
            },
        ]
    );
    let texture = image::open(directory.path().join(TEXTURE)).unwrap();
    assert_eq!(texture.color(), image::ColorType::Rgb8);
    let texture = texture.into_rgba8();
    assert_eq!(texture.dimensions(), (16, 4));
    assert_eq!(texture.get_pixel(0, 0)[0], 128);
    assert_eq!(texture.get_pixel(0, 0)[1], u8::MAX);
    assert_eq!(texture.get_pixel(0, 0)[2], 64);
    let preview =
        image::open(directory.path().join(skwd_wall_core::infrastructure::drawing::PREVIEW))
            .unwrap();
    assert_eq!(preview.color(), image::ColorType::La8);
    assert_eq!(preview.to_luma_alpha8().get_pixel(0, 0)[1], 128);
}

#[test]
fn preview_fits_edge() {
    let directory = tempfile::tempdir().unwrap();
    let farther = Plane::filled(8, 4, 0.25);
    let ink = vec![255; 960 * 480];
    let drawing = Drawing {
        strokes: &[trace()],
        events: &[event(0, 0.0, 1.0)],
        farther: &farther,
        matte: &Plane::filled(960, 480, 0.0),
        ink: &ink,
        ink_size: [960, 480],
    };
    drawing.write(directory.path()).unwrap();
    let preview =
        image::open(directory.path().join(skwd_wall_core::infrastructure::drawing::PREVIEW))
            .unwrap();
    assert_eq!((preview.width(), preview.height()), (320, 160));
}

#[test]
fn rejects_outline_size() {
    let farther = Plane::filled(8, 4, 0.0);
    let drawing = Drawing {
        strokes: &[],
        events: &[],
        farther: &farther,
        matte: &farther,
        ink: &[0; 128],
        ink_size: [16, 8],
    };
    let directory = tempfile::tempdir().unwrap();
    assert!(drawing.write(directory.path()).is_err());
}
