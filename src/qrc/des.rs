// Compatibility port of WXRIW/QQMusicDecoder at 0e149419; see LICENSE-QRC.
use super::tables::*;

const QRC_KEY: &[u8; 24] = b"!@#)(*$%123ZXC!@!@#)(NHL";

fn permute(value: u64, bits: u8, indices: &[u8]) -> u64 {
    indices.iter().fold(0, |result, index| {
        (result << 1) | ((value >> (bits - 1 - index)) & 1)
    })
}

fn word_order(mut bytes: [u8; 8]) -> [u8; 8] {
    bytes[..4].reverse();
    bytes[4..].reverse();
    bytes
}

fn schedule(key: [u8; 8], decrypt: bool) -> [u64; 16] {
    let key = u64::from_be_bytes(word_order(key));
    let mut c = (permute(key, 64, &KEY_PERMUTATION_C) as u32) << 4;
    let mut d = (permute(key, 64, &KEY_PERMUTATION_D) as u32) << 4;
    let mut result = [0; 16];
    for (round, shift) in ROUND_SHIFTS.into_iter().enumerate() {
        c = ((c << shift) | (c >> (28 - shift))) & 0xfffffff0;
        d = ((d << shift) | (d >> (28 - shift))) & 0xfffffff0;
        result[if decrypt { 15 - round } else { round }] =
            KEY_COMPRESSION.iter().fold(0, |value, &index| {
                // The reference uses -27, NOT the standard DES -28, for the D half.
                let bit = if index < 28 {
                    (c >> (31 - index)) & 1
                } else {
                    (d >> (31 - (index - 27))) & 1
                };
                (value << 1) | u64::from(bit)
            });
    }
    result
}

fn round_function(value: u32, key: u64) -> u32 {
    let expanded = permute(u64::from(value), 32, &EXPANSION) ^ key;
    let mut substituted = 0;
    for (index, table) in S_BOXES.iter().enumerate() {
        let value = ((expanded >> (42 - index * 6)) & 63) as usize;
        let row = ((value & 32) >> 4) | (value & 1);
        let column = (value >> 1) & 15;
        substituted = (substituted << 4) | u32::from(table[row * 16 + column]);
    }
    permute(u64::from(substituted), 32, &ROUND_PERMUTATION) as u32
}

fn crypt_block(block: [u8; 8], keys: &[u64; 16]) -> [u8; 8] {
    let initial = permute(
        u64::from_be_bytes(word_order(block)),
        64,
        &INITIAL_PERMUTATION,
    );
    let (mut left, mut right) = ((initial >> 32) as u32, initial as u32);
    for &key in keys {
        (left, right) = (right, left ^ round_function(right, key));
    }
    let swapped = (u64::from(right) << 32) | u64::from(left);
    let mut result = 0;
    for (index, &position) in INITIAL_PERMUTATION.iter().enumerate() {
        result |= ((swapped >> (63 - index)) & 1) << (63 - position);
    }
    word_order(result.to_be_bytes())
}

pub(super) fn decrypt_blocks(bytes: &mut [u8]) {
    let keys = [
        schedule(QRC_KEY[16..24].try_into().unwrap(), true),
        schedule(QRC_KEY[8..16].try_into().unwrap(), false),
        schedule(QRC_KEY[..8].try_into().unwrap(), true),
    ];
    for block in bytes.as_chunks_mut::<8>().0 {
        for schedule in &keys {
            *block = crypt_block(*block, schedule);
        }
    }
}
