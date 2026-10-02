use aes::Aes128;
use blowfish::Blowfish;
use cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use md5::{Digest, Md5};

const CHUNK: usize = 2048;
const SECRET: &[u8; 16] = b"g4el58wc0zvf9na1";
const IV: [u8; 8] = [0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07];
const CDN_KEY: [u8; 16] = *b"jo6aey6haid2Teih";
const HEX: &[u8; 16] = b"0123456789abcdef";

type StripeCipher = Blowfish;

fn latin1(value: &str) -> Vec<u8> {
    value.chars().map(|ch| ch as u8).collect()
}

fn md5_digest(value: &str) -> [u8; 16] {
    let mut hasher = Md5::new();
    if value.is_ascii() {
        hasher.update(value.as_bytes());
    } else {
        hasher.update(latin1(value));
    }
    hasher.finalize().into()
}

fn md5_hex(value: &str) -> String {
    let digest = md5_digest(value);
    let mut hex = String::with_capacity(32);
    push_hex(&mut hex, &digest);
    hex
}

fn push_hex(out: &mut String, bytes: &[u8]) {
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
    }
}

pub fn blowfish_key(track_id: &str) -> [u8; 16] {
    let digest = md5_digest(track_id);
    let mut hex = [0u8; 32];
    for (index, byte) in digest.iter().enumerate() {
        hex[index * 2] = HEX[(byte >> 4) as usize];
        hex[index * 2 + 1] = HEX[(byte & 0xf) as usize];
    }
    let mut key = [0u8; 16];
    for index in 0..16 {
        key[index] = hex[index] ^ hex[index + 16] ^ SECRET[index];
    }
    key
}

fn stripe_cipher(track_id: &str) -> StripeCipher {
    Blowfish::new_from_slice(&blowfish_key(track_id)).expect("16-byte blowfish key")
}

/// One Blowfish schedule for the whole file. Each 2048-byte stripe is its own
/// CBC chain and starts again from `IV`, so the schedule is reused and only
/// the chaining value resets.
fn crypt_stripe(cipher: &StripeCipher, stripe: &mut [u8], encrypt: bool) {
    debug_assert_eq!(stripe.len(), CHUNK);
    let mut prev = IV;
    if encrypt {
        for block in stripe.chunks_exact_mut(8) {
            for index in 0..8 {
                block[index] ^= prev[index];
            }
            let mut out = cipher::generic_array::GenericArray::clone_from_slice(block);
            cipher.encrypt_block(&mut out);
            block.copy_from_slice(&out);
            prev.copy_from_slice(block);
        }
    } else {
        for block in stripe.chunks_exact_mut(8) {
            let ciphertext: [u8; 8] = block.try_into().expect("blowfish block");
            let mut out = cipher::generic_array::GenericArray::clone_from_slice(&ciphertext);
            cipher.decrypt_block(&mut out);
            for index in 0..8 {
                block[index] = out[index] ^ prev[index];
            }
            prev = ciphertext;
        }
    }
}

fn crypt_stripes_in_place(buf: &mut [u8], cipher: &StripeCipher, start_chunk: u64, encrypt: bool) {
    let mut offset = 0;
    let mut chunk_index = start_chunk;
    while offset < buf.len() {
        let size = (buf.len() - offset).min(CHUNK);
        if chunk_index % 3 == 0 && size == CHUNK {
            crypt_stripe(cipher, &mut buf[offset..offset + CHUNK], encrypt);
        }
        offset += size;
        chunk_index += 1;
    }
}

pub fn decrypt_download(source: &[u8], track_id: &str) -> Vec<u8> {
    let mut output = source.to_vec();
    crypt_stripes_in_place(&mut output, &stripe_cipher(track_id), 0, false);
    output
}

pub fn encrypt_download(source: &[u8], track_id: &str) -> Vec<u8> {
    let mut output = source.to_vec();
    crypt_stripes_in_place(&mut output, &stripe_cipher(track_id), 0, true);
    output
}

pub struct TrackDecryptor {
    cipher: StripeCipher,
    carry: Vec<u8>,
    chunk_index: u64,
}

impl TrackDecryptor {
    pub fn new(track_id: &str, start_chunk: u64) -> Self {
        Self {
            cipher: stripe_cipher(track_id),
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
        crypt_stripes_in_place(&mut self.carry[..complete], &self.cipher, self.chunk_index, false);
        self.chunk_index += (complete / CHUNK) as u64;
        let tail = self.carry.split_off(complete);
        std::mem::replace(&mut self.carry, tail)
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
    while step2.chars().count() % 16 != 0 {
        step2.push(' ');
    }
    let cipher = Aes128::new((&CDN_KEY).into());
    let input = latin1(&step2);
    let mut hex = String::with_capacity(input.len() * 2);
    for block in input.chunks(16) {
        let mut block = cipher::generic_array::GenericArray::clone_from_slice(block);
        cipher.encrypt_block(&mut block);
        push_hex(&mut hex, &block);
    }
    Ok(hex)
}

pub fn legacy_cdn_url(md5_origin: &str, quality: i64, song_id: &str, media_version: &str) -> Result<String, super::error::DeezerError> {
    let filename = song_file_name(md5_origin, quality, song_id, media_version)?;
    let shard = md5_origin.chars().next().unwrap_or('0');
    Ok(format!("https://e-cdns-proxy-{shard}.dzcdn.net/mobile/1/{filename}"))
}
