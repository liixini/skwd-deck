use super::Config;

impl Config {
    skwd_config::getters! {
        weather_enabled: off_unless_on(skwd_config::keys::weather::ENABLED);
        weather_match: off_unless_on(skwd_config::keys::general::WEATHER_MATCH);
    }

    pub fn weather_playlist(&self, category: &str) -> Option<i64> {
        use skwd_config::keys::weather;
        use wall_proto::weather_category;
        let key = match category {
            weather_category::CLEAR => weather::CLEAR,
            weather_category::CLOUDY => weather::CLOUDY,
            weather_category::RAINY => weather::RAINY,
            weather_category::SNOWY => weather::SNOWY,
            weather_category::STORMY => weather::STORMY,
            weather_category::FOGGY => weather::FOGGY,
            weather_category::WINDY => weather::WINDY,
            _ => return None,
        };
        self.get(key)?.as_str()?.trim().parse::<i64>().ok().filter(|id| *id > 0)
    }

    pub fn weather_source(&self) -> wall_proto::WeatherSource {
        use skwd_config::keys::weather;
        match skwd_config::schema::read_text(&self.root, weather::SOURCE).as_deref() {
            Some(weather::SOURCE_OUTDOORS) => wall_proto::WeatherSource::Outdoors,
            Some(weather::SOURCE_SEMANTIC) => wall_proto::WeatherSource::Semantic,
            _ => wall_proto::WeatherSource::Playlists,
        }
    }

    pub fn outdoors_contrast_margin(&self) -> f32 {
        skwd_config::schema::read_number(
            &self.root,
            skwd_config::keys::weather::OUTDOORS_CONTRAST_MARGIN,
        )
        .expect("outdoors score setting") as f32
    }

    pub fn outdoors_dwell_seconds(&self) -> u64 {
        skwd_config::schema::read_number(
            &self.root,
            skwd_config::keys::weather::OUTDOORS_DWELL_SECONDS,
        )
        .expect("outdoors dwell setting") as u64
    }

    pub fn outdoors_transition_shader(&self) -> String {
        let key = skwd_config::keys::weather::OUTDOORS_TRANSITION_SHADER;
        let selected =
            skwd_config::schema::read_text(&self.root, key).expect("outdoors transition setting");
        if wall_proto::transitions::TRANSITIONS.iter().any(|transition| transition.key == selected)
        {
            selected
        } else {
            skwd_config::schema::text_default(key).expect("outdoors transition default").to_owned()
        }
    }

    pub fn outdoors_retheme(&self) -> bool {
        skwd_config::schema::read_boolean(&self.root, skwd_config::keys::weather::OUTDOORS_RETHEME)
            .expect("outdoors retheme setting")
    }

    pub fn outdoors_stable_seconds(&self) -> u64 {
        skwd_config::schema::read_number(
            &self.root,
            skwd_config::keys::weather::OUTDOORS_STABLE_SECONDS,
        )
        .expect("outdoors stability setting") as u64
    }

    pub fn weather_selection(&self) -> wall_proto::WeatherSelection {
        use skwd_config::keys::weather;
        if skwd_config::schema::read_text(&self.root, weather::SEMANTIC_SELECTION).as_deref()
            == Some(weather::SELECTION_APPROVED)
        {
            wall_proto::WeatherSelection::Approved
        } else {
            wall_proto::WeatherSelection::Automatic
        }
    }

    pub fn weather_approved(&self, category: &str) -> Vec<String> {
        let Some(key) = skwd_config::keys::weather::approved_key(category) else {
            return Vec::new();
        };
        self.get(key)
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| item.as_str().map(str::to_owned))
            .collect()
    }

    pub fn weather_semantic_query(&self, category: &str) -> Option<String> {
        use skwd_config::keys::weather;
        use wall_proto::weather_category;
        let key = match category {
            weather_category::CLEAR => weather::SEMANTIC_CLEAR,
            weather_category::CLOUDY => weather::SEMANTIC_CLOUDY,
            weather_category::RAINY => weather::SEMANTIC_RAINY,
            weather_category::SNOWY => weather::SEMANTIC_SNOWY,
            weather_category::STORMY => weather::SEMANTIC_STORMY,
            weather_category::FOGGY => weather::SEMANTIC_FOGGY,
            weather_category::WINDY => weather::SEMANTIC_WINDY,
            _ => return None,
        };
        let query = skwd_config::schema::read_text(&self.root, key)?;
        let query = query.trim();
        (!query.is_empty()).then(|| query.to_owned())
    }

    pub fn weather_semantic_max_results(&self) -> usize {
        skwd_config::schema::read_number(
            &self.root,
            skwd_config::keys::weather::SEMANTIC_MAX_RESULTS,
        )
        .expect("semantic max results setting") as usize
    }

    pub fn weather_semantic_score_window(&self) -> f32 {
        skwd_config::schema::read_number(
            &self.root,
            skwd_config::keys::weather::SEMANTIC_SCORE_WINDOW,
        )
        .expect("semantic score window setting") as f32
    }

    pub fn weather_semantic_prominence(&self) -> f32 {
        skwd_config::schema::read_number(
            &self.root,
            skwd_config::keys::weather::SEMANTIC_MIN_SCORE_PROMINENCE,
        )
        .expect("semantic prominence setting") as f32
    }

    pub fn weather_semantic_dwell_seconds(&self) -> u64 {
        skwd_config::schema::read_number(
            &self.root,
            skwd_config::keys::weather::SEMANTIC_DWELL_SECONDS,
        )
        .expect("semantic dwell setting") as u64
    }
}
