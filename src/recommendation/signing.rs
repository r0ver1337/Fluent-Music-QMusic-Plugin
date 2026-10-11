use base64::Engine;
use ring::hmac;
use zeroize::Zeroizing;

// Verified protocol constants from sample SHA-256 fe07be49...3e7b228a; no ELF is bundled or read.
pub(super) fn material() -> Zeroizing<[u8; 12]> {
    let words = Zeroizing::new([
        0xe152795eu32,
        0xd0143f67,
        0xd7164068,
        0x66e29653,
        0xd2134f6a,
    ]);
    let omit = ((words[0] % 10) % 3 + 1) as usize;
    let mut key = Zeroizing::new([0u8; 12]);
    let mut offset = 0;
    for index in 1..5 {
        if index != omit {
            key[offset..offset + 4].copy_from_slice(&(words[0] ^ words[index]).to_le_bytes());
            offset += 4;
        }
    }
    key
}
pub(super) fn tea(block: [u8; 8], key: &[u8; 16]) -> [u8; 8] {
    let mut a = u32::from_be_bytes(block[..4].try_into().unwrap());
    let mut b = u32::from_be_bytes(block[4..].try_into().unwrap());
    let k: Zeroizing<[u32; 4]> = Zeroizing::new(std::array::from_fn(|i| {
        u32::from_be_bytes(key[i * 4..i * 4 + 4].try_into().unwrap())
    }));
    let mut sum = 0u32;
    for _ in 0..16 {
        sum = sum.wrapping_add(0x9e3779b9);
        a = a.wrapping_add(
            (b << 4).wrapping_add(k[0]) ^ b.wrapping_add(sum) ^ (b >> 5).wrapping_add(k[1]),
        );
        b = b.wrapping_add(
            (a << 4).wrapping_add(k[2]) ^ a.wrapping_add(sum) ^ (a >> 5).wrapping_add(k[3]),
        );
    }
    let mut out = [0; 8];
    out[..4].copy_from_slice(&a.to_be_bytes());
    out[4..].copy_from_slice(&b.to_be_bytes());
    out
}
pub(super) fn signature(
    body: &[u8],
    metadata: &[u8],
    key: &[u8; 12],
    random: &[u8; 32],
) -> (String, String) {
    let mut message = base64::engine::general_purpose::STANDARD
        .encode(&body[..body.len().min(51200)])
        .into_bytes();
    message.reverse();
    let digest = hmac::sign(
        &hmac::Key::new(hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY, key),
        &message,
    );
    let mut combined = Vec::with_capacity(32);
    combined.extend(random[..12].iter().map(|b| b'A' + b % 26));
    combined.extend_from_slice(digest.as_ref());
    let sign = base64::engine::general_purpose::STANDARD.encode(combined);
    let mut tea_key = Zeroizing::new([0u8; 16]);
    tea_key[..8].copy_from_slice(&sign.as_bytes()[..8]);
    tea_key[8..].copy_from_slice(&sign.as_bytes()[sign.len() - 8..]);
    let pad = (8 - (metadata.len() + 10) % 8) % 8;
    let mut plain = Zeroizing::new(Vec::with_capacity(metadata.len() + pad + 10));
    plain.push((random[12] & 0xf8) | pad as u8);
    plain.extend_from_slice(&random[13..13 + pad + 2]);
    plain.extend_from_slice(metadata);
    plain.extend_from_slice(&[0; 7]);
    let mut previous_cipher = [0u8; 8];
    let mut previous_x = [0u8; 8];
    let mut output = Vec::with_capacity(plain.len());
    for block in plain.as_chunks::<8>().0 {
        let mut x = [0; 8];
        for i in 0..8 {
            x[i] = block[i] ^ previous_cipher[i];
        }
        let encrypted = tea(x, &tea_key);
        let mut cipher = [0; 8];
        for i in 0..8 {
            cipher[i] = encrypted[i] ^ previous_x[i];
        }
        output.extend_from_slice(&cipher);
        previous_x = x;
        previous_cipher = cipher;
    }
    (
        sign,
        base64::engine::general_purpose::STANDARD.encode(output),
    )
}
