#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioFormatId {
    Mp3,
    Flac,
    Aac,
    M4a,
    Ogg,
    Wav,
    Webm,
}

impl AudioFormatId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mp3 => "mp3",
            Self::Flac => "flac",
            Self::Aac => "aac",
            Self::M4a => "m4a",
            Self::Ogg => "ogg",
            Self::Wav => "wav",
            Self::Webm => "webm",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFormat {
    pub id: AudioFormatId,
    pub label: &'static str,
    pub content_type: &'static str,
    pub extensions: &'static [&'static str],
    pub mime_types: &'static [&'static str],
}

pub const AUDIO_FORMATS: &[AudioFormat] = &[
    AudioFormat {
        id: AudioFormatId::Mp3,
        label: "MP3",
        content_type: "audio/mpeg",
        extensions: &["mp3"],
        mime_types: &["audio/mpeg", "audio/mp3", "audio/x-mpeg"],
    },
    AudioFormat {
        id: AudioFormatId::Flac,
        label: "FLAC",
        content_type: "audio/flac",
        extensions: &["flac"],
        mime_types: &["audio/flac", "audio/x-flac"],
    },
    AudioFormat {
        id: AudioFormatId::Aac,
        label: "AAC",
        content_type: "audio/aac",
        extensions: &["aac"],
        mime_types: &["audio/aac", "audio/x-aac"],
    },
    AudioFormat {
        id: AudioFormatId::M4a,
        label: "M4A",
        content_type: "audio/mp4",
        extensions: &["m4a", "mp4"],
        mime_types: &["audio/mp4", "audio/x-m4a"],
    },
    AudioFormat {
        id: AudioFormatId::Ogg,
        label: "Ogg",
        content_type: "audio/ogg",
        extensions: &["ogg", "oga"],
        mime_types: &["audio/ogg", "application/ogg"],
    },
    AudioFormat {
        id: AudioFormatId::Wav,
        label: "WAV",
        content_type: "audio/wav",
        extensions: &["wav"],
        mime_types: &["audio/wav", "audio/wave", "audio/x-wav"],
    },
    AudioFormat {
        id: AudioFormatId::Webm,
        label: "WebM",
        content_type: "audio/webm",
        extensions: &["webm"],
        mime_types: &["audio/webm"],
    },
];

pub fn audio_accept() -> String {
    let mut parts = Vec::new();
    for format in AUDIO_FORMATS {
        for mime in format.mime_types {
            parts.push((*mime).to_string());
        }
        for extension in format.extensions {
            parts.push(format!(".{extension}"));
        }
    }
    parts.join(",")
}

fn format(id: AudioFormatId) -> AudioFormat {
    *AUDIO_FORMATS
        .iter()
        .find(|candidate| candidate.id == id)
        .expect("audio format registry entry")
}

fn starts_with(bytes: &[u8], signature: &[u8], offset: usize) -> bool {
    bytes
        .get(offset..offset + signature.len())
        .is_some_and(|slice| slice == signature)
}

fn includes_ascii(bytes: &[u8], value: &str, limit: usize) -> bool {
    let encoded = value.as_bytes();
    if encoded.is_empty() || bytes.len() < encoded.len() {
        return false;
    }
    let end = bytes.len().saturating_sub(encoded.len()).saturating_add(1).min(limit);
    (0..end).any(|offset| starts_with(bytes, encoded, offset))
}

pub fn detect_audio_format(bytes: &[u8]) -> Option<AudioFormat> {
    if starts_with(bytes, b"fLaC", 0) {
        return Some(format(AudioFormatId::Flac));
    }
    if starts_with(bytes, b"OggS", 0) {
        return Some(format(AudioFormatId::Ogg));
    }

    let is_wave = (starts_with(bytes, b"RIFF", 0)
        || starts_with(bytes, b"RF64", 0)
        || starts_with(bytes, b"BW64", 0))
        && starts_with(bytes, b"WAVE", 8);
    if is_wave {
        return Some(format(AudioFormatId::Wav));
    }

    if starts_with(bytes, b"ftyp", 4) {
        return Some(format(AudioFormatId::M4a));
    }

    if starts_with(bytes, &[0x1a, 0x45, 0xdf, 0xa3], 0) && includes_ascii(bytes, "webm", 4096) {
        return Some(format(AudioFormatId::Webm));
    }

    if starts_with(bytes, b"ID3", 0) {
        return Some(format(AudioFormatId::Mp3));
    }

    if bytes.len() >= 2 && bytes[0] == 0xff {
        let second = bytes[1];
        if second & 0xf6 == 0xf0 {
            return Some(format(AudioFormatId::Aac));
        }
        let version = (second >> 3) & 0b11;
        let layer = (second >> 1) & 0b11;
        if second & 0xe0 == 0xe0 && version != 0b01 && layer != 0b00 {
            return Some(format(AudioFormatId::Mp3));
        }
    }

    None
}

pub fn find_audio_format_by_extension(file_name: &str) -> Option<AudioFormat> {
    let trimmed = file_name.trim();
    let extension = trimmed.rsplit('.').next()?.to_ascii_lowercase();
    if extension.is_empty() || extension == trimmed.to_ascii_lowercase() {
        return None;
    }
    AUDIO_FORMATS
        .iter()
        .find(|candidate| candidate.extensions.iter().any(|item| *item == extension))
        .copied()
}

pub fn find_audio_format_by_mime_type(mime_type: &str) -> Option<AudioFormat> {
    let normalized = mime_type
        .trim()
        .to_ascii_lowercase()
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if normalized.is_empty() {
        return None;
    }
    AUDIO_FORMATS
        .iter()
        .find(|candidate| candidate.mime_types.iter().any(|item| *item == normalized))
        .copied()
}
