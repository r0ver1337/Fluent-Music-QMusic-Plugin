use super::*;

pub enum Stream {
    Map([u8; 128]),
    Rc4 {
        key: Vec<u8>,
        hash: f64,
        stream: Vec<u8>,
    },
}

impl Stream {
    pub fn from_ekey(ekey: &str) -> Result<Self, ApiError> {
        Self::from_master(derive_master(ekey)?)
    }

    fn from_master(key: Vec<u8>) -> Result<Self, ApiError> {
        if key.is_empty() {
            return Err(key_error());
        }
        if key.len() <= 300 {
            let map = std::array::from_fn(|i| {
                let index = (i * i + 71214) % key.len();
                let shift = (index + 4) % 8;
                // This is deliberately NOT an ordinary rotate-left operation.
                (((key[index] as u16) << shift) | ((key[index] as u16) >> shift)) as u8
            });
            return Ok(Self::Map(map));
        }
        let n = key.len();
        let mut state: Vec<u8> = (0..n).map(|i| i as u8).collect();
        let mut j = 0;
        for i in 0..n {
            j = (j + state[i] as usize + key[i] as usize) % n;
            state.swap(i, j);
        }
        let (mut i, mut j) = (0, 0);
        let stream = (0..SEGMENT + 512)
            .map(|_| {
                i = (i + 1) % n;
                j = (j + state[i] as usize) % n;
                state.swap(i, j);
                state[(state[i] as usize + state[j] as usize) % n]
            })
            .collect();
        let mut hash = 1u32;
        for &value in &key {
            if value == 0 {
                continue;
            }
            let next = hash.wrapping_mul(value as u32);
            if next == 0 || next <= hash {
                break;
            }
            hash = next;
        }
        Ok(Self::Rc4 {
            key,
            hash: hash as f64,
            stream,
        })
    }

    /// Absolute offsets make chunked conversion identical to whole-file conversion.
    pub fn apply(&self, bytes: &mut [u8], offset: u64) {
        match self {
            Self::Map(key) => {
                for (i, byte) in bytes.iter_mut().enumerate() {
                    let pos = offset + i as u64;
                    let pos = if pos > BOUNDARY { pos % BOUNDARY } else { pos };
                    *byte ^= key[(pos % 128) as usize];
                }
            }
            Self::Rc4 { key, hash, stream } => {
                let mut pos = offset;
                let mut start = 0;
                let segment_key = |id: u64, seed: u8| -> u64 {
                    if seed == 0 {
                        0
                    } else {
                        (hash / (id + 1).wrapping_mul(seed as u64) as f64 * 100.0) as u64
                    }
                };
                while start < bytes.len() && pos < 128 {
                    let index = segment_key(pos, key[pos as usize % key.len()]);
                    bytes[start] ^= key[index as usize % key.len()];
                    start += 1;
                    pos += 1;
                }
                while start < bytes.len() {
                    let id = pos / SEGMENT as u64;
                    let block_offset = (pos % SEGMENT as u64) as usize;
                    let skip = (segment_key(id, key[id as usize % key.len()]) & 511) as usize;
                    let take = (SEGMENT - block_offset).min(bytes.len() - start);
                    for (byte, mask) in bytes[start..start + take]
                        .iter_mut()
                        .zip(&stream[skip + block_offset..skip + block_offset + take])
                    {
                        *byte ^= mask;
                    }
                    start += take;
                    pos += take as u64;
                }
            }
        }
    }
}
