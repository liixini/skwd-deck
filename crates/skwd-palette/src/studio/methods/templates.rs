use super::super::analysis::{Analysis, Cluster};
use super::super::oklch::{FULL_TURN, hue_distance, oklch};
use super::super::polish::WALLPAPER_TERMINAL;
use super::{Draft, Made, Pair, Shape, finish, grey_theory, strength, theory_neutrals};

#[derive(Debug, Clone, Copy, PartialEq)]
struct Sector {
    centre: f32,
    width: f32,
}

const fn sector(centre: f32, width: f32) -> Sector {
    Sector { centre, width }
}

const TEMPLATES: [&[Sector]; 7] = [
    &[sector(0.0, 18.0)],
    &[sector(0.0, 93.6)],
    &[sector(0.0, 18.0), sector(90.0, 79.2)],
    &[sector(0.0, 18.0), sector(180.0, 18.0)],
    &[sector(0.0, 180.0)],
    &[sector(0.0, 93.6), sector(180.0, 18.0)],
    &[sector(0.0, 93.6), sector(180.0, 93.6)],
];
const ROTATION_STEP: usize = 2;
const WIDTH_COST: f32 = 0.06;
const NEAR_REACH: f32 = 20.0;
const SAME_SECTOR_SHIFT: f32 = 2.5;
const THIRD_SHIFT: f32 = 3.0;
const REACH_CHROMA: f32 = 0.15;
const NEUTRAL_CHROMA: f32 = 0.012;
const HALF_TURN: f32 = 180.0;
const ACCENT: Pair = Pair { dark: 0.78, light: 0.5 };
const SECOND: Pair = Pair { dark: 0.74, light: 0.48 };
const THIRD: Pair = Pair { dark: 0.84, light: 0.42 };
const ACCENT_CHROMA: f32 = 0.15;
const SECOND_CHROMA: f32 = 0.12;
const THIRD_CHROMA: f32 = 0.08;

fn cost(found: &[Cluster], sectors: &[Sector], rotation: f32) -> f32 {
    let (sum, total) = found.iter().fold((0.0f32, 0.0f32), |(sum, total), cluster| {
        let gap = sectors
            .iter()
            .map(|sector| {
                (hue_distance(cluster.lch.hue, (sector.centre + rotation).rem_euclid(FULL_TURN))
                    - sector.width / 2.0)
                    .max(0.0)
            })
            .fold(f32::MAX, f32::min);
        (sum + strength(cluster) * gap, total + strength(cluster))
    });
    sum / total
}

fn clamp_into(hue: f32, sector: Sector) -> f32 {
    let offset = (hue - sector.centre + HALF_TURN).rem_euclid(FULL_TURN) - HALF_TURN;
    (sector.centre + offset.clamp(-sector.width / 2.0, sector.width / 2.0)).rem_euclid(FULL_TURN)
}

fn first_max(found: &[Cluster]) -> Option<Cluster> {
    found.iter().copied().fold(None, |best, cluster| match best {
        Some(kept) if strength(&kept) >= strength(&cluster) => Some(kept),
        _ => Some(cluster),
    })
}

pub(super) fn make(analysis: &Analysis, dark: bool) -> Made {
    let found: Vec<Cluster> = analysis.chromatic().copied().collect();
    let Some(peak) = first_max(&found).filter(|_| !analysis.grey) else {
        return grey_theory(analysis, dark);
    };
    let mut best: Option<(f32, usize, f32)> = None;
    for (index, sectors) in TEMPLATES.iter().enumerate() {
        let width: f32 = sectors.iter().map(|sector| sector.width).sum();
        for rotation in (0..FULL_TURN as usize).step_by(ROTATION_STEP) {
            let rotation = rotation as f32;
            let score = cost(&found, sectors, rotation) + WIDTH_COST * width;
            if best.is_none_or(|(kept, _, _)| score < kept) {
                best = Some((score, index, rotation));
            }
        }
    }
    let (_, index, rotation) = best.unwrap_or((0.0, 0, 0.0));
    let sectors: Vec<Sector> = TEMPLATES[index]
        .iter()
        .map(|sector| Sector {
            centre: (sector.centre + rotation).rem_euclid(FULL_TURN),
            ..*sector
        })
        .collect();
    let main_index = sectors
        .iter()
        .enumerate()
        .fold(None, |best: Option<(usize, f32)>, (slot, sector)| {
            let gap = hue_distance(peak.lch.hue, sector.centre);
            match best {
                Some((_, kept)) if kept <= gap => best,
                _ => Some((slot, gap)),
            }
        })
        .map_or(0, |(slot, _)| slot);
    let main = sectors[main_index];
    let peak_hue = clamp_into(peak.lch.hue, main);
    let other =
        sectors.iter().enumerate().find(|(slot, _)| *slot != main_index).map(|(_, sector)| *sector);
    let second_hue = match other {
        Some(sector) => {
            let near: Vec<Cluster> = found
                .iter()
                .copied()
                .filter(|cluster| {
                    hue_distance(cluster.lch.hue, sector.centre) <= sector.width / 2.0 + NEAR_REACH
                })
                .collect();
            first_max(&near).map_or(sector.centre, |cluster| clamp_into(cluster.lch.hue, sector))
        }
        None => clamp_into(peak_hue + main.width / SAME_SECTOR_SHIFT, main),
    };
    let third_hue = (main.centre - main.width / THIRD_SHIFT).rem_euclid(FULL_TURN);
    let strongest = found.iter().map(|cluster| cluster.lch.chroma).fold(0.0, f32::max);
    let reach = (strongest / REACH_CHROMA).min(1.0);
    let neutrals = theory_neutrals(main.centre, NEUTRAL_CHROMA, dark);
    let draft = Draft {
        neutrals,
        accent: oklch(ACCENT.pick(dark), ACCENT_CHROMA * reach, peak_hue),
        on_accent: neutrals[0],
        second: oklch(SECOND.pick(dark), SECOND_CHROMA * reach, second_hue),
        third: oklch(THIRD.pick(dark), THIRD_CHROMA * reach, third_hue),
    };
    Made {
        palette: finish(&draft, analysis, dark, &WALLPAPER_TERMINAL),
        mark: None,
        shape: Shape::Sectors(sectors.iter().map(|sector| [sector.centre, sector.width]).collect()),
    }
}
