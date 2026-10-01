use aes::Aes128;
use blowfish::Blowfish;
use cbc::{Decryptor, Encryptor};
use cipher::block_padding::NoPadding;
use cipher::{BlockDecryptMut, BlockEncryptMut, BlockEncrypt, KeyInit, KeyIvInit};
use md5::{Digest, Md5};

const CHUNK: usize = 2048;
const SECRET: &[u8; 16] = b"g4el58wc0zvf9na1";
const IV: [u8; 8] = [0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07];
const CDN_KEY: &[u8; 16] = b"jo6aey6haid2Teih";

type BfDec = Decryptor<Blowfish>;
type BfEnc = Encryptor<Blowfish>;

fn latin1(value: &str) -> Vec<u8> {
    value.chars().map(|ch| ch as u8).collect()
}

fn md5_hex(value: &str) -> String {
    let digest = Md5::digest(latin1(value));
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn blowfish_key(track_id: &str) -> [u8; 16] {
    let hex = md5_hex(track_id);
    let bytes = hex.as_bytes();
    let mut key = [0u8; 16];
    for index in 0..16 {
        key[index] = bytes[index] ^ bytes[index + 16] ^ SECRET[index];
    }
    key
}

fn crypt_chunk(key: &[u8; 16], chunk: &[u8], encrypt: bool) -> Vec<u8> {
    let mut buffer = chunk.to_vec();
    if encrypt {
        let enc = BfEnc::new_from_slices(key, &IV).expect("blowfish key");
        enc.encrypt_padded_mut::<NoPadding>(&mut buffer, chunk.len())
            .expect("aligned stripe")
            .to_vec()
    } else {
        let dec = BfDec::new_from_slices(key, &IV).expect("blowfish key");
        dec.decrypt_padded_mut::<NoPadding>(&mut buffer)
            .expect("aligned stripe")
            .to_vec()
    }
}

fn apply_stripes(source: &[u8], key: &[u8; 16], start_chunk: u64, encrypt: bool) -> Vec<u8> {
    let mut output = Vec::with_capacity(source.len());
    let mut offset = 0;
    let mut chunk_index = start_chunk;
    while offset < source.len() {
        let size = (source.len() - offset).min(CHUNK);
        let slice = &source[offset..offset + size];
        if chunk_index % 3 == 0 && size == CHUNK {
            output.extend(crypt_chunk(key, slice, encrypt));
        } else {
            output.extend_from_slice(slice);
        }
        offset += size;
        chunk_index += 1;
    }
    output
}

pub fn decrypt_download(source: &[u8], track_id: &str) -> Vec<u8> {
    apply_stripes(source, &blowfish_key(track_id), 0, false)
}

pub fn encrypt_download(source: &[u8], track_id: &str) -> Vec<u8> {
    apply_stripes(source, &blowfish_key(track_id), 0, true)
}

pub struct TrackDecryptor {
    key: [u8; 16],
    carry: Vec<u8>,
    chunk_index: u64,
}

impl TrackDecryptor {
    pub fn new(track_id: &str, start_chunk: u64) -> Self {
        Self {
            key: blowfish_key(track_id),
            carry: Vec::new(),
            chunk_index: start_chunk,
        }
    }

    pub fn push(&mut self, part: &[u8]) -> Vec<u8> {
        self.carry.extend_from_slice(part);
        let complete = self.carry.len() - (self.carry.len() % CHUNK);
        if complete == 0 {
            return Vec::new();
        }
        let ready = self.carry[..complete].to_vec();
        self.carry.drain(..complete);
        let plain = apply_stripes(&ready, &self.key, self.chunk_index, false);
        self.chunk_index += (complete / CHUNK) as u64;
        plain
    }

    pub fn finish(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.carry)
    }
}

pub fn song_file_name(md5_origin: &str, quality: i64, song_id: &str, media_version: &str) -> Result<String, super::error::DeezerError> {
    if md5_origin.is_empty() {
        return Err(super::error::DeezerError::Message(format!(
            "Missing MD5_ORIGIN for track {song_id}"
        )));
    }
    let step1 = format!("{md5_origin}\u{00a4}{quality}\u{00a4}{song_id}\u{00a4}{media_version}");
    let mut step2 = format!("{}\u{00a4}{step1}\u{00a4}", md5_hex(&step1));
    while latin1(&step2).len() % 16 != 0 {
        step2.push(' ');
    }
    let cipher = Aes128::new((&CDN_KEY[..]).into());
    let input = latin1(&step2);
    let mut hex = String::new();
    for block in input.chunks(16) {
        let mut block = cipher::generic_array::GenericArray::clone_from_slice(block);
        cipher.encrypt_block(&mut block);
        for byte in block {
            hex.push_str(&format!("{byte:02x}"));
        }
    }
    Ok(hex)
}

pub fn legacy_cdn_url(md5_origin: &str, quality: i64, song_id: &str, media_version: &str) -> Result<String, super::error::DeezerError> {
    let filename = song_file_name(md5_origin, quality, song_id, media_version)?;
    let shard = md5_origin.chars().next().unwrap_or('0');
    Ok(format!("https://e-cdns-proxy-{shard}.dzcdn.net/mobile/1/{filename}"))
}
