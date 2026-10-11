use super::*;

pub(super) fn tea_mix(value: u32, sum: u32, k1: u32, k2: u32) -> u32 {
    (value << 4).wrapping_add(k1) ^ (value >> 5).wrapping_add(k2) ^ sum.wrapping_add(value)
}

pub(super) fn tea_block(block: u64, words: &[u32; 4]) -> u64 {
    let mut hi = (block >> 32) as u32;
    let mut lo = block as u32;
    let mut sum = 0x9e3779b9u32.wrapping_mul(16);
    for _ in 0..16 {
        lo = lo.wrapping_sub(tea_mix(hi, sum, words[2], words[3]));
        hi = hi.wrapping_sub(tea_mix(lo, sum, words[0], words[1]));
        sum = sum.wrapping_sub(0x9e3779b9);
    }
    ((hi as u64) << 32) | lo as u64
}

pub(super) fn tea_cbc(cipher: &[u8], key: &[u8; 16]) -> Result<Vec<u8>, ApiError> {
    if cipher.len() < 16 || !cipher.len().is_multiple_of(8) {
        return Err(key_error());
    }
    let words =
        std::array::from_fn(|i| u32::from_be_bytes(key[i * 4..i * 4 + 4].try_into().unwrap()));
    let mut plain = Vec::with_capacity(cipher.len());
    let (mut previous, mut current) = (0, 0);
    for chunk in cipher.as_chunks::<8>().0 {
        let block = u64::from_be_bytes(*chunk);
        let next = tea_block(block ^ current, &words);
        plain.extend_from_slice(&(next ^ previous).to_be_bytes());
        previous = block;
        current = next;
    }
    let start = 1 + (plain[0] & 7) as usize + 2;
    let end = plain.len() - 7;
    if start > end || plain[end..].iter().any(|v| *v != 0) {
        return Err(key_error());
    }
    Ok(plain[start..end].to_vec())
}

pub(super) fn derive_v1(ekey: &[u8]) -> Result<Vec<u8>, ApiError> {
    let decoded = STANDARD.decode(ekey).map_err(|_| key_error())?;
    if decoded.len() < 24 {
        return Err(key_error());
    }
    let mut tea_key = [0; 16];
    for i in 0..8 {
        tea_key[i * 2] = SIMPLE_KEY[i];
        tea_key[i * 2 + 1] = decoded[i];
    }
    let mut master = decoded[..8].to_vec();
    master.extend(tea_cbc(&decoded[8..], &tea_key)?);
    Ok(master)
}

pub(super) fn derive_master(ekey: &str) -> Result<Vec<u8>, ApiError> {
    if ekey.len() > MAX_EKEY_BYTES {
        return Err(key_error());
    }
    let ekey = ekey.trim().trim_end_matches('\0');
    if let Some(payload) = ekey.strip_prefix(EKEY_V2_PREFIX) {
        let payload = STANDARD.decode(payload).map_err(|_| key_error())?;
        let payload = tea_cbc(&payload, EKEY_V2_KEY1)?;
        let payload = tea_cbc(&payload, EKEY_V2_KEY2)?;
        let end = payload
            .iter()
            .position(|v| *v == 0)
            .unwrap_or(payload.len());
        derive_v1(&payload[..end])
    } else {
        derive_v1(ekey.as_bytes())
    }
}
