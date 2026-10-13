use std::collections::HashMap;

use skwd_lens_proto::{SearchRequest, SearchResponse};
use wall_proto::{LightPhase, OutdoorSeason, weather_category};
use wall_rules::outdoors::Context;

use super::{SemanticPaths, search_requests};

#[derive(Clone, Copy)]
enum Facet {
    Nature,
    Other,
    Night,
    Day,
    Snow,
    Bare,
    Summer,
    Autumn,
    Cloudy,
    Clear,
    Spring,
    Rain,
    Fog,
    Storm,
    Dawn,
    Dusk,
}

const FACETS: [(Facet, &str); 16] = [
    (Facet::Nature, "A natural outdoor landscape with trees, lakes, mountains and sky"),
    (Facet::Other, "An indoor room, an animal portrait, a cartoon character or abstract shapes"),
    (Facet::Night, "A dark landscape at night with stars and moonlight"),
    (Facet::Day, "A bright landscape during daylight"),
    (Facet::Snow, "A snow-covered winter landscape"),
    (Facet::Bare, "A landscape of leafless trees, brown vegetation and exposed ground"),
    (Facet::Summer, "A summer landscape with lush green foliage"),
    (Facet::Autumn, "An autumn landscape with yellow orange and red leaves"),
    (Facet::Cloudy, "An overcast landscape under cloudy gray skies"),
    (Facet::Clear, "A sunny landscape under clear blue skies"),
    (Facet::Spring, "A pond in spring with fresh green foliage"),
    (Facet::Rain, "A rainy landscape with wet ground and falling rain"),
    (Facet::Fog, "A foggy landscape with mist obscuring trees and distant scenery"),
    (Facet::Storm, "A stormy landscape with dark thunderclouds and lightning"),
    (Facet::Dawn, "A landscape at dawn with soft early sunrise light"),
    (Facet::Dusk, "A landscape at dusk with soft sunset twilight"),
];
const NEGATIVE_WEIGHT: f32 = 0.5;

fn request(query: String, count: usize, generation: usize) -> SearchRequest {
    SearchRequest {
        generation: generation as u64,
        query,
        negative_query: None,
        negative_weight: NEGATIVE_WEIGHT,
        top_k: count,
        score_window: None,
        min_score_prominence: None,
        max_results: None,
        min_results: 0,
        embedding_only: false,
    }
}

pub(super) fn search(
    paths: &SemanticPaths,
    context: &Context,
    count: usize,
    margin: f32,
) -> Result<Vec<String>, String> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let mut primary = request(context.query(), count, 0);
    primary.negative_query = Some(context.negative_query());
    let mut requests = vec![primary];
    requests.extend(
        FACETS
            .iter()
            .enumerate()
            .map(|(index, (_, query))| request((*query).to_owned(), count, index + 1)),
    );
    let scores = search_requests(paths, &requests, Scores::default(), Scores::append)?;
    Ok(scores.select(context, margin))
}

#[derive(Default)]
struct Scores {
    order: Vec<String>,
    facets: HashMap<String, [f32; FACETS.len()]>,
}

impl Scores {
    fn append(&mut self, response: SearchResponse) {
        if response.generation == 0 {
            for item in response.matches {
                self.facets.insert(item.key.clone(), [f32::NAN; FACETS.len()]);
                self.order.push(item.key);
            }
            return;
        }
        let facet = response.generation as usize - 1;
        for item in response.matches {
            if let Some(scores) = self.facets.get_mut(&item.key) {
                scores[facet] = item.score;
            }
        }
    }

    fn select(self, context: &Context, margin: f32) -> Vec<String> {
        self.order
            .into_iter()
            .filter(|key| {
                self.facets
                    .get(key)
                    .is_some_and(|scores| fits(context, margin, |facet| scores[facet as usize]))
            })
            .collect()
    }
}

fn fits(context: &Context, margin: f32, score: impl Fn(Facet) -> f32) -> bool {
    use Facet::{
        Autumn, Bare, Clear, Cloudy, Dawn, Day, Dusk, Fog, Nature, Night, Other, Rain, Snow,
        Spring, Storm, Summer,
    };
    let beats = |wanted, opposite| {
        let wanted = score(wanted);
        let opposite = score(opposite);
        wanted.is_finite() && opposite.is_finite() && wanted - opposite >= margin
    };
    let night = matches!(context.light, LightPhase::Night | LightPhase::Dusk);
    let light = match context.light {
        LightPhase::Night => beats(Night, Day),
        LightPhase::Day => beats(Day, Night),
        LightPhase::Dawn => beats(Dawn, Day) && beats(Dawn, Night),
        LightPhase::Dusk => beats(Dusk, Day) && beats(Dusk, Night),
    };
    if !beats(Nature, Other)
        || !light
        || !if context.snow { beats(Snow, Nature) } else { beats(Nature, Snow) }
    {
        return false;
    }
    if !night && !context.snow {
        let season = match context.season {
            OutdoorSeason::Neutral => true,
            OutdoorSeason::Spring => beats(Spring, Bare) && beats(Spring, Autumn),
            OutdoorSeason::Summer => beats(Summer, Bare) && beats(Summer, Autumn),
            OutdoorSeason::Autumn => beats(Autumn, Summer) && beats(Autumn, Bare),
            OutdoorSeason::Winter => beats(Bare, Summer) && beats(Bare, Autumn),
        };
        if !season {
            return false;
        }
    }
    match context.category.as_str() {
        weather_category::CLOUDY => beats(Cloudy, Clear),
        weather_category::CLEAR => beats(Clear, Cloudy),
        weather_category::RAINY => beats(Rain, Clear) && beats(Rain, Cloudy),
        weather_category::FOGGY => beats(Fog, Clear) && beats(Fog, Cloudy),
        weather_category::STORMY => beats(Storm, Clear) && beats(Storm, Cloudy),
        weather_category::SNOWY => context.snow && beats(Cloudy, Clear),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
