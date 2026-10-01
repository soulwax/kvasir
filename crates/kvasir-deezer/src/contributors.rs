use serde_json::Value;

use crate::track::Track;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Person {
    pub role: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NormalizedContributors {
    pub main_artists: Vec<String>,
    pub featuring: Vec<String>,
    pub composers: Vec<String>,
    pub lyricists: Vec<String>,
    pub producers: Vec<String>,
    pub engineers: Vec<Person>,
    pub mixers: Vec<String>,
    pub performers: Vec<Person>,
    pub publishers: Vec<String>,
}

fn canon(key: &str) -> String {
    key.chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '_' && *ch != '-')
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn uniq(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() && !out.contains(&trimmed) {
            out.push(trimmed);
        }
    }
    out
}

const ENGINEER_LABELS: &[(&str, &str)] = &[
    ("masteringengineer", "mastering engineer"),
    ("mixingengineer", "mixing engineer"),
    ("recordingengineer", "recording engineer"),
    ("recordingsecondengineer", "assistant recording engineer"),
    ("assistantengineer", "assistant engineer"),
    ("engineer", "engineer"),
    ("studiopersonnel", "engineer"),
];

pub fn normalize_contributors(raw: &Value) -> NormalizedContributors {
    let Some(object) = raw.as_object() else {
        return NormalizedContributors::default();
    };
    let mut bucket: Vec<(String, Vec<String>)> = Vec::new();
    for (key, value) in object {
        let Some(names) = value.as_array() else {
            continue;
        };
        let names = names
            .iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect::<Vec<_>>();
        let key = canon(key);
        if let Some((_, existing)) = bucket.iter_mut().find(|(candidate, _)| candidate == &key) {
            existing.extend(names);
        } else {
            bucket.push((key, names));
        }
    }
    let take = |name: &str| {
        bucket
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, values)| values.clone())
            .unwrap_or_default()
    };
    let mut engineers = Vec::new();
    for (key, label) in ENGINEER_LABELS {
        for name in uniq(take(key)) {
            engineers.push(Person {
                role: (*label).to_string(),
                name,
            });
        }
    }
    let mut performers = Vec::new();
    for (key, names) in &bucket {
        if matches!(
            key.as_str(),
            "mainartist" | "artist" | "featuring" | "featuredartist" | "feat" | "composer" | "mixer" | "remixer" | "producer" | "coproducer" | "executiveproducer"
        ) || ENGINEER_LABELS.iter().any(|(candidate, _)| candidate == key)
            || key.contains("publisher")
            || key.contains("author")
            || key.contains("writer")
            || key.contains("lyricist")
        {
            continue;
        }
        for name in uniq(names.clone()) {
            performers.push(Person {
                role: key.clone(),
                name,
            });
        }
    }
    NormalizedContributors {
        main_artists: uniq(take("mainartist").into_iter().chain(take("artist"))),
        featuring: uniq(take("featuring").into_iter().chain(take("featuredartist")).chain(take("feat"))),
        composers: uniq(take("composer")),
        lyricists: uniq(take("author").into_iter().chain(take("writer")).chain(take("lyricist")).chain(take("songwriter"))),
        producers: uniq(take("producer").into_iter().chain(take("coproducer")).chain(take("executiveproducer"))),
        engineers,
        mixers: uniq(take("mixer").into_iter().chain(take("remixer"))),
        performers,
        publishers: uniq(take("publisher").into_iter().chain(take("musicpublisher")).chain(take("originalpublisher"))),
    }
}

pub fn append_version(track: &mut Track) {
    let version = track.version();
    if !version.is_empty() && !track.title().contains(&version) {
        let title = format!("{} {version}", track.title());
        if let Some(object) = track.raw.as_object_mut() {
            object.insert("SNG_TITLE".into(), serde_json::Value::String(title));
        }
    }
}
