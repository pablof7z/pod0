use zeroize::Zeroizing;

const INITIAL_STATE: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

const ROUND_CONSTANTS: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

pub(crate) fn sha256(parts: &[&[u8]]) -> Zeroizing<[u8; 32]> {
    let mut hasher = SecureSha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize()
}

struct SecureSha256 {
    state: Zeroizing<[u32; 8]>,
    buffer: Zeroizing<[u8; 64]>,
    buffered: usize,
    byte_length: u64,
}

impl SecureSha256 {
    fn new() -> Self {
        Self {
            state: Zeroizing::new(INITIAL_STATE),
            buffer: Zeroizing::new([0; 64]),
            buffered: 0,
            byte_length: 0,
        }
    }

    fn update(&mut self, mut input: &[u8]) {
        self.byte_length = self
            .byte_length
            .checked_add(input.len() as u64)
            .expect("SHA-256 input length overflow");
        while !input.is_empty() {
            let count = (64 - self.buffered).min(input.len());
            self.buffer[self.buffered..self.buffered + count].copy_from_slice(&input[..count]);
            self.buffered += count;
            input = &input[count..];
            if self.buffered == 64 {
                Self::compress(&mut self.state, &self.buffer);
                self.buffer.fill(0);
                self.buffered = 0;
            }
        }
    }

    fn finalize(mut self) -> Zeroizing<[u8; 32]> {
        let bit_length = self.byte_length.checked_mul(8).expect("SHA-256 bit length");
        self.buffer[self.buffered] = 0x80;
        self.buffered += 1;
        if self.buffered > 56 {
            self.buffer[self.buffered..].fill(0);
            Self::compress(&mut self.state, &self.buffer);
            self.buffer.fill(0);
            self.buffered = 0;
        }
        self.buffer[self.buffered..56].fill(0);
        self.buffer[56..].copy_from_slice(&bit_length.to_be_bytes());
        Self::compress(&mut self.state, &self.buffer);

        let mut output = Zeroizing::new([0; 32]);
        for (chunk, word) in output.chunks_exact_mut(4).zip(self.state.iter()) {
            chunk.copy_from_slice(&word.to_be_bytes());
        }
        output
    }

    fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
        let mut schedule = Zeroizing::new([0_u32; 64]);
        for (word, chunk) in schedule.iter_mut().zip(block.chunks_exact(4)) {
            *word = u32::from_be_bytes(chunk.try_into().expect("four-byte SHA-256 word"));
        }
        for index in 16..64 {
            schedule[index] = schedule[index - 16]
                .wrapping_add(
                    schedule[index - 15].rotate_right(7)
                        ^ schedule[index - 15].rotate_right(18)
                        ^ (schedule[index - 15] >> 3),
                )
                .wrapping_add(schedule[index - 7])
                .wrapping_add(
                    schedule[index - 2].rotate_right(17)
                        ^ schedule[index - 2].rotate_right(19)
                        ^ (schedule[index - 2] >> 10),
                );
        }

        let mut working = Zeroizing::new(*state);
        let mut terms = Zeroizing::new([0_u32; 2]);
        for (constant, word) in ROUND_CONSTANTS.iter().zip(schedule.iter()) {
            terms[0] = working[7]
                .wrapping_add(
                    working[4].rotate_right(6)
                        ^ working[4].rotate_right(11)
                        ^ working[4].rotate_right(25),
                )
                .wrapping_add((working[4] & working[5]) ^ ((!working[4]) & working[6]))
                .wrapping_add(*constant)
                .wrapping_add(*word);
            terms[1] = (working[0].rotate_right(2)
                ^ working[0].rotate_right(13)
                ^ working[0].rotate_right(22))
            .wrapping_add(
                (working[0] & working[1])
                    ^ (working[0] & working[2])
                    ^ (working[1] & working[2]),
            );
            working.copy_within(4..7, 5);
            working[4] = working[3].wrapping_add(terms[0]);
            working.copy_within(0..3, 1);
            working[0] = terms[0].wrapping_add(terms[1]);
        }
        for (state, value) in state.iter_mut().zip(working.iter()) {
            *state = state.wrapping_add(*value);
        }
    }
}

#[cfg(test)]
mod tests {
    use sha2::{Digest as _, Sha256};

    use super::*;

    #[test]
    fn secure_sha256_matches_reference_across_block_boundaries() {
        for input in [
            Vec::new(),
            b"abc".to_vec(),
            vec![0x5a; 55],
            vec![0xa5; 56],
            vec![0x3c; 64],
            vec![0xc3; 129],
        ] {
            let expected: [u8; 32] = Sha256::digest(&input).into();
            assert_eq!(*sha256(&[&input[..input.len() / 2], &input[input.len() / 2..]]), expected);
        }
    }
}
