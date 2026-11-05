use super::*;
use crate::models::Bounds;
use crate::plane::Plane;

const SIZE: [usize; 2] = [200, 100];

fn line(start: Point, end: Point) -> Stroke {
    Stroke { points: vec![start, end], width: 7.0, kind: Kind::Trace }
}

fn left_half(head: Option<Bounds>) -> Subject {
    let mut mask = Plane::filled(SIZE[0], SIZE[1], 0.0);
    for y in 0..SIZE[1] {
        for x in 0..SIZE[0] / 2 {
            mask.data[y * SIZE[0] + x] = 1.0;
        }
    }
    Subject::new(head, mask, [50.0, 10.0])
}

fn scene() -> Vec<Stroke> {
    let mut strokes: Vec<Stroke> = (0..10)
        .flat_map(|step| {
            let x = step as f32 * 9.0 + 5.0;
            [line([x, 30.0], [x, 90.0]), line([x + 110.0, 30.0], [x + 110.0, 90.0])]
        })
        .collect();
    strokes.push(line([40.0, 5.0], [60.0, 15.0]));
    strokes.push(Stroke {
        points: vec![[150.0, 50.0], [160.0, 40.0]],
        width: 24.0,
        kind: Kind::Hatch,
    });
    strokes
}

const HEAD: Bounds = Bounds { left: 35.0, top: 0.0, right: 65.0, bottom: 20.0 };

#[test]
fn every_stroke_once() {
    let strokes = scene();
    let mut planned: Vec<usize> =
        plan(&strokes, &left_half(Some(HEAD))).iter().map(|event| event.stroke).collect();
    planned.sort_unstable();
    assert_eq!(planned, (0..strokes.len()).collect::<Vec<_>>());
}

#[test]
fn subject_before_world() {
    let strokes = scene();
    for event in plan(&strokes, &left_half(Some(HEAD))) {
        let x = raster::centre(&strokes[event.stroke].points)[0];
        let window = if x < 100.0 { [0.0, PLAN.subject_end] } else { [PLAN.world_start, 1.0] };
        assert!(event.start >= window[0] && event.end <= window[1] + 1e-5);
        assert!(event.start <= event.end);
    }
}

#[test]
fn head_leads_body() {
    let strokes = scene();
    let events = plan(&strokes, &left_half(Some(HEAD)));
    let head_start = events.iter().find(|event| event.stroke == 20).unwrap().start;
    let body_start = events
        .iter()
        .filter(|event| event.stroke != 20 && event.end <= PLAN.subject_end)
        .map(|event| event.start)
        .fold(f32::MAX, f32::min);
    assert_eq!(head_start, 0.0);
    assert!(body_start >= PLAN.subject_end * PLAN.head_lead - 1e-5);
}

#[test]
fn world_fills_alone() {
    let strokes = scene();
    let subject = Subject::new(None, Plane::filled(SIZE[0], SIZE[1], 0.0), [0.0, 0.0]);
    let events = plan(&strokes, &subject);
    assert!(events.iter().any(|event| event.start < PLAN.world_start));
    let last = events.iter().map(|event| event.end).fold(0.0, f32::max);
    assert!((last - 1.0).abs() < 1e-5);
}

#[test]
fn chain_follows_ends() {
    let strokes = [
        line([20.0, 0.0], [30.0, 0.0]),
        line([0.0, 0.0], [10.0, 0.0]),
        line([10.0, 0.0], [20.0, 0.0]),
    ];
    let planner = Planner {
        strokes: &strokes,
        centres: strokes.iter().map(|stroke| raster::centre(&stroke.points)).collect(),
        lengths: strokes.iter().map(|stroke| raster::length(&stroke.points)).collect(),
        start: [0.0, 0.0],
    };
    assert_eq!(planner.chain(vec![0, 1, 2]), vec![1, 2, 0]);
}

#[test]
fn edge_strokes_join_subject() {
    let strokes = [line([103.0, 30.0], [103.0, 90.0]), line([180.0, 30.0], [180.0, 90.0])];
    let events = plan(&strokes, &left_half(None));
    assert!(events[0].end <= PLAN.subject_end + 1e-5);
    assert!(events[1].start >= PLAN.world_start);
}

#[test]
fn hatch_only_subject() {
    let strokes = [
        Stroke { points: vec![[20.0, 50.0], [30.0, 40.0]], width: 24.0, kind: Kind::Hatch },
        line([180.0, 30.0], [180.0, 90.0]),
    ];
    let events = plan(&strokes, &left_half(None));
    let last = events.iter().map(|event| event.end).fold(0.0, f32::max);
    assert!(events.iter().all(|event| event.start >= 0.0));
    assert!((last - 1.0).abs() < 1e-5);
    assert!(events.iter().any(|event| event.stroke == 1 && event.start < PLAN.world_start));
}
