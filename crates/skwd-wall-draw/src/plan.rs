use skwd_palette::seed;

use crate::raster::{self, Point};
use crate::strokes::{Kind, Stroke};
use crate::subject::{INSIDE, Subject};

struct PlanSpec {
    samples: usize,
    head_share: f32,
    edge_share: f32,
    subject_share: f32,
    subject_end: f32,
    world_start: f32,
    head_pens: usize,
    subject_pens: usize,
    world_pens: usize,
    head_end: f32,
    head_lead: f32,
    contour_percent: usize,
    contour_end: f32,
    detail_delay: f32,
    hatch_start: f32,
    passes: usize,
    chain_bonus: f32,
    min_length: f32,
}

const PLAN: PlanSpec = PlanSpec {
    samples: 8,
    head_share: 0.5,
    edge_share: 0.5,
    subject_share: 0.35,
    subject_end: 0.5,
    world_start: 0.48,
    head_pens: 2,
    subject_pens: 5,
    world_pens: 9,
    head_end: 0.22,
    head_lead: 0.18,
    contour_percent: 65,
    contour_end: 0.62,
    detail_delay: 0.15,
    hatch_start: 0.35,
    passes: 8,
    chain_bonus: 0.15,
    min_length: 10.0,
};

const PERCENT: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Head,
    Body,
    World,
}

#[derive(Clone, Debug)]
pub struct Event {
    pub stroke: usize,
    pub start: f32,
    pub end: f32,
}

struct Block {
    head: Vec<usize>,
    traces: Vec<usize>,
    hatches: Vec<usize>,
    window: [f32; 2],
    pens: usize,
}

struct Planner<'a> {
    strokes: &'a [Stroke],
    centres: Vec<Point>,
    lengths: Vec<f32>,
    start: Point,
}

fn part(stroke: &Stroke, subject: &Subject) -> Part {
    let step = (stroke.points.len() / PLAN.samples).max(1);
    let samples: Vec<Point> = stroke.points.iter().step_by(step).copied().collect();
    let fraction = |inside: &dyn Fn(Point) -> bool| {
        samples.iter().filter(|point| inside(**point)).count() as f32 / samples.len() as f32
    };
    if subject.head.is_some_and(|head| fraction(&|point| head.contains(point)) >= PLAN.head_share) {
        return Part::Head;
    }
    if fraction(&|point| subject.edge.sample(point) > INSIDE) >= PLAN.edge_share
        || fraction(&|point| subject.mask.sample(point) > INSIDE) >= PLAN.subject_share
    {
        return Part::Body;
    }
    Part::World
}

pub fn plan(strokes: &[Stroke], subject: &Subject) -> Vec<Event> {
    let parts: Vec<Part> = strokes.iter().map(|stroke| part(stroke, subject)).collect();
    let pick = |wanted: &[Part], kind: Kind| -> Vec<usize> {
        (0..strokes.len())
            .filter(|&index| strokes[index].kind == kind && wanted.contains(&parts[index]))
            .collect()
    };
    let traced = |wanted: fn(Part) -> bool| {
        strokes.iter().zip(&parts).any(|(stroke, part)| stroke.kind == Kind::Trace && wanted(*part))
    };
    let has_world = traced(|part| part == Part::World);
    let has_subject = traced(|part| part != Part::World);
    let (subject_window, world_window) = if has_world && has_subject {
        ([0.0, PLAN.subject_end], [PLAN.world_start, 1.0])
    } else {
        ([0.0, 1.0], [0.0, 1.0])
    };
    let planner = Planner {
        strokes,
        centres: strokes.iter().map(|stroke| raster::centre(&stroke.points)).collect(),
        lengths: strokes.iter().map(|stroke| raster::length(&stroke.points)).collect(),
        start: subject.start,
    };
    let mut events = planner.block(Block {
        head: pick(&[Part::Head], Kind::Trace),
        traces: pick(&[Part::Body], Kind::Trace),
        hatches: pick(&[Part::Head, Part::Body], Kind::Hatch),
        window: subject_window,
        pens: PLAN.subject_pens,
    });
    events.extend(planner.block(Block {
        head: Vec::new(),
        traces: pick(&[Part::World], Kind::Trace),
        hatches: pick(&[Part::World], Kind::Hatch),
        window: world_window,
        pens: PLAN.world_pens,
    }));
    events
}

impl Planner<'_> {
    fn block(&self, block: Block) -> Vec<Event> {
        let [from, to] = block.window;
        let span = to - from;
        let lead = if block.head.is_empty() { 0.0 } else { span * PLAN.head_lead };
        let mut sizes: Vec<f32> =
            block.head.iter().chain(&block.traces).map(|&index| self.lengths[index]).collect();
        sizes.sort_by(f32::total_cmp);
        let cut = sizes.get(sizes.len() * PLAN.contour_percent / PERCENT).copied().unwrap_or(0.0);
        let (contours, details): (Vec<usize>, Vec<usize>) =
            block.traces.iter().partition(|&&index| self.lengths[index] >= cut);
        let passes = [
            (block.head, [from, from + span * PLAN.head_end], PLAN.head_pens),
            (contours, [from + lead, from + span * PLAN.contour_end], block.pens),
            (details, [from + lead + span * PLAN.detail_delay, to], block.pens * 2),
            (block.hatches, [from + span * PLAN.hatch_start, to], block.pens * 2),
        ];
        passes
            .into_iter()
            .flat_map(|(strokes, window, pens)| self.regions(&strokes, window, pens))
            .map(|(stroke, start, end)| Event { stroke, start, end })
            .collect()
    }

    fn regions(&self, strokes: &[usize], window: [f32; 2], pens: usize) -> Vec<(usize, f32, f32)> {
        if strokes.is_empty() {
            return Vec::new();
        }
        let chains: Vec<Vec<usize>> = self
            .clusters(strokes, pens.min(strokes.len()))
            .into_iter()
            .filter(|group| !group.is_empty())
            .map(|group| self.chain(group))
            .collect();
        let length = |index: usize| self.lengths[index].max(PLAN.min_length);
        let busiest = chains
            .iter()
            .map(|chain| chain.iter().map(|&index| length(index)).sum::<f32>())
            .fold(f32::EPSILON, f32::max);
        let span = window[1] - window[0];
        let mut slots = Vec::with_capacity(strokes.len());
        for chain in chains {
            let mut clock = 0.0;
            for index in chain {
                let start = window[0] + clock / busiest * span;
                clock += length(index);
                slots.push((index, start, window[0] + clock / busiest * span));
            }
        }
        slots
    }

    fn nearest(&self, strokes: &[usize], to: Point) -> usize {
        (0..strokes.len())
            .min_by(|left, right| {
                raster::distance(self.centres[strokes[*left]], to)
                    .total_cmp(&raster::distance(self.centres[strokes[*right]], to))
            })
            .expect("strokes to choose from")
    }

    fn clusters(&self, strokes: &[usize], count: usize) -> Vec<Vec<usize>> {
        let points: Vec<seed::Point> = strokes
            .iter()
            .map(|&index| (self.centres[index][0], self.centres[index][1], 0.0))
            .collect();
        let weights: Vec<f32> = strokes.iter().map(|&index| self.lengths[index].max(1.0)).collect();
        let found = seed::cluster(&points, &weights, count, PLAN.passes);
        let mut groups = vec![Vec::new(); found.centers.len()];
        for (&index, &owner) in strokes.iter().zip(&found.owner) {
            groups[owner].push(index);
        }
        groups
    }

    fn chain(&self, mut left: Vec<usize>) -> Vec<usize> {
        let ends = |index: usize| {
            let points = &self.strokes[index].points;
            (points[0], points[points.len() - 1])
        };
        let mut chain = vec![left.swap_remove(self.nearest(&left, self.start))];
        let mut pen = ends(chain[0]).1;
        while !left.is_empty() {
            let reach = |slot: usize| {
                let (head, tail) = ends(left[slot]);
                let (near, far) = if raster::distance(pen, head) <= raster::distance(pen, tail) {
                    (head, tail)
                } else {
                    (tail, head)
                };
                (raster::distance(pen, near) - self.lengths[left[slot]] * PLAN.chain_bonus, far)
            };
            let slot = (0..left.len())
                .min_by(|first, second| reach(*first).0.total_cmp(&reach(*second).0))
                .expect("strokes left");
            pen = reach(slot).1;
            chain.push(left.swap_remove(slot));
        }
        chain
    }
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
