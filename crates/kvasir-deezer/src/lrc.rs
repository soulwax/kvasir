use serde_json::Value;

#[derive(Clone, Debug, Default)]
pub struct LrcMeta {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub writers: Option<String>,
    pub length_seconds: Option<u64>,
}

fn stamp(ms: u64) -> String {
    let cs = (ms as f64 / 10.0).round() as u64;
    let minutes = cs / 6000;
    let seconds = (cs % 6000) / 100;
    let centis = cs % 100;
    format!("{minutes:02}:{seconds:02}.{centis:02}")
}

pub fn to_lrc(sync: Option<&Value>, meta: &LrcMeta) -> Option<String> {
    let lines = sync?.as_array()?;
    if lines.is_empty() {
        return None;
    }
    let mut head = Vec::new();
    if let Some(artist) = &meta.artist {
        head.push(format!("[ar:{artist}]"));
    }
    if let Some(title) = &meta.title {
        head.push(format!("[ti:{title}]"));
    }
    if let Some(album) = &meta.album {
        head.push(format!("[al:{album}]"));
    }
    if let Some(writers) = &meta.writers {
        head.push(format!("[au:{writers}]"));
    }
    if let Some(length) = meta.length_seconds {
        head.push(format!("[length:{}]", stamp(length * 1000)));
    }
    head.push("[re:kvasir]".into());
    let body: Vec<String> = lines
        .iter()
        .filter_map(|line| {
            let text = line.get("line").and_then(Value::as_str).unwrap_or("").trim();
            if text.is_empty() {
                return None;
            }
            let timestamp = line
                .get("milliseconds")
                .and_then(|value| value.as_u64().or_else(|| value.as_str().and_then(|text| text.parse().ok())))
                .map(|ms| format!("[{}]", stamp(ms)))
                .or_else(|| line.get("lrc_timestamp").and_then(Value::as_str).map(str::to_string))
                .unwrap_or_default();
            Some(format!("{timestamp}{text}"))
        })
        .collect();
    Some(format!("{}\n", head.into_iter().chain(body).collect::<Vec<_>>().join("\n")))
}
