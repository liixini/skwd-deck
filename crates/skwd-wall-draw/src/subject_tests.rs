use super::*;

const SIZE: [usize; 2] = [100, 60];

fn blob(mask: &mut Plane, [left, top, right, bottom]: [usize; 4]) {
    for y in top..bottom {
        for x in left..right {
            mask.data[y * mask.width + x] = 1.0;
        }
    }
}

fn found(matte: Plane) -> Found {
    Found { faces: Vec::new(), heads: Vec::new(), matte, detections: Vec::new() }
}

fn bounds([left, top, right, bottom]: [f32; 4]) -> Bounds {
    Bounds { left, top, right, bottom }
}

#[test]
fn face_sets_start() {
    let mut character = Plane::filled(SIZE[0], SIZE[1], 0.0);
    blob(&mut character, [20, 10, 40, 60]);
    blob(&mut character, [70, 10, 90, 30]);
    let subject = find(
        &Found { faces: vec![bounds([25.0, 10.0, 35.0, 20.0])], ..found(character) },
        &Plane::filled(SIZE[0], SIZE[1], 0.5),
    );
    assert_eq!(subject.start, [30.0, 14.2]);
    assert_eq!(subject.mask.at(30, 50), 1.0);
    assert_eq!(subject.mask.at(80, 20), 0.0);
    assert!(subject.head.is_some());
}

#[test]
fn animal_near_depth() {
    let mut farther = Plane::filled(SIZE[0], SIZE[1], 0.9);
    for y in 7..55 {
        for x in 25..78 {
            farther.data[y * SIZE[0] + x] = 0.1;
        }
    }
    let detection = Detection { confidence: 0.9, bounds: bounds([20.0, 5.0, 80.0, 55.0]) };
    let subject = find(
        &Found { detections: vec![detection], ..found(Plane::filled(SIZE[0], SIZE[1], 0.0)) },
        &farther,
    );
    assert_eq!(subject.mask.at(50, 30), 1.0);
    assert_eq!(subject.mask.at(22, 7), 0.0);
    assert_eq!(subject.start[0], 50.0);
}

#[test]
fn crown_without_face() {
    let mut character = Plane::filled(SIZE[0], SIZE[1], 0.0);
    blob(&mut character, [40, 20, 60, 60]);
    let subject = find(&found(character), &Plane::filled(SIZE[0], SIZE[1], 0.5));
    assert!(subject.head.is_none());
    assert_eq!(subject.start[1], 22.0);
    assert!((subject.start[0] - 49.5).abs() < 1e-4);
}

#[test]
fn centre_without_subject() {
    let subject =
        find(&found(Plane::filled(SIZE[0], SIZE[1], 0.0)), &Plane::filled(SIZE[0], SIZE[1], 0.5));
    assert_eq!(subject.start, [50.0, 30.0]);
    assert_eq!(share(&subject.mask), 0.0);
}

#[test]
fn head_matches_face() {
    let mut character = Plane::filled(SIZE[0], SIZE[1], 0.0);
    blob(&mut character, [20, 10, 40, 60]);
    blob(&mut character, [70, 10, 90, 60]);
    let subject = find(
        &Found {
            faces: vec![bounds([25.0, 10.0, 35.0, 20.0])],
            heads: vec![bounds([72.0, 8.0, 88.0, 24.0]), bounds([22.0, 8.0, 38.0, 24.0])],
            ..found(character)
        },
        &Plane::filled(SIZE[0], SIZE[1], 0.5),
    );
    assert_eq!(subject.head.unwrap().left, 22.0);
    assert_eq!(subject.mask.at(80, 40), 0.0);
}

#[test]
fn body_below_lone_head() {
    let mut character = Plane::filled(SIZE[0], SIZE[1], 0.0);
    blob(&mut character, [80, 40, 95, 55]);
    let subject = find(
        &Found { faces: vec![bounds([25.0, 10.0, 35.0, 20.0])], ..found(character) },
        &Plane::filled(SIZE[0], SIZE[1], 0.5),
    );
    assert_eq!(subject.mask.at(30, 50), 1.0);
    assert_eq!(subject.mask.at(30, 5), 0.0);
    assert_eq!(subject.mask.at(90, 50), 0.0);
}

#[test]
fn edge_rings_mask() {
    let mut mask = Plane::filled(SIZE[0], SIZE[1], 0.0);
    blob(&mut mask, [30, 10, 70, 50]);
    let band = edge(&mask);
    assert_eq!(band.at(30, 30), 1.0);
    assert_eq!(band.at(50, 30), 0.0);
    assert_eq!(band.at(5, 30), 0.0);
}

#[test]
fn matte_falls_back() {
    let mut salient = Plane::filled(SIZE[0], SIZE[1], 0.0);
    salient.data[0] = 1.0;
    let empty = Plane::filled(SIZE[0], SIZE[1], 0.0);
    let matte = matte(empty, || Ok::<_, ()>(salient)).unwrap();
    assert_eq!(matte.data[0], 1.0);
}

#[test]
fn character_skips_salient() {
    let mut character = Plane::filled(SIZE[0], SIZE[1], 0.0);
    blob(&mut character, [20, 10, 40, 60]);
    let matte = matte(character, || Err("salient ran")).unwrap();
    assert_eq!(matte.at(30, 30), 1.0);
}

#[test]
fn outline_is_hard() {
    let mut character = Plane::filled(SIZE[0], SIZE[1], 0.0);
    blob(&mut character, [20, 10, 60, 40]);
    character.data[2 * SIZE[0] + 2] = 0.9;
    let guide = Plane::filled(SIZE[0] * 2, SIZE[1] * 2, 0.5);
    let cut = outline(&found(character), &guide);
    assert_eq!([cut.width, cut.height], [SIZE[0] * 2, SIZE[1] * 2]);
    assert!(cut.data.iter().all(|&value| value == 0.0 || value == 1.0));
    assert_eq!(cut.at(80, 50), 1.0);
    assert_eq!(cut.at(4, 4), 0.0);
}

#[test]
fn outline_fills_pinholes() {
    let mut character = Plane::filled(SIZE[0], SIZE[1], 0.0);
    blob(&mut character, [20, 10, 60, 40]);
    character.data[25 * SIZE[0] + 40] = 0.0;
    let cut = outline(&found(character), &Plane::filled(SIZE[0], SIZE[1], 0.5));
    assert_eq!(cut.at(40, 25), 1.0);
}
