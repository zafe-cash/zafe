use bip0039::{English, Language};
use zafe_proto::{IdentityPublic, MailboxId};

pub const WORDS: usize = 4;

fn hash(personal: &[u8; 16], parts: &[&[u8]]) -> [u8; 32] {
    let mut state = blake2b_simd::Params::new()
        .hash_length(32)
        .personal(personal)
        .to_state();
    for p in parts {
        state.update(p);
    }
    state.finalize().as_bytes().try_into().expect("32 bytes")
}

pub fn commitment(mailbox: &MailboxId, sig_pk: &[u8; 32], secret: &[u8; 32]) -> [u8; 32] {
    hash(b"Zafe_SasCommit__", &[mailbox, sig_pk, secret])
}

pub fn words(mailbox: &MailboxId, members: &[(IdentityPublic, [u8; 32])]) -> String {
    let mut sorted: Vec<&(IdentityPublic, [u8; 32])> = members.iter().collect();
    sorted.sort_by_key(|m| m.0);
    let mut parts: Vec<Vec<u8>> = vec![
        mailbox.to_vec(),
        (sorted.len() as u32).to_le_bytes().to_vec(),
    ];
    for (identity, value) in sorted {
        parts.push(identity.fingerprint().to_vec());
        parts.push(value.to_vec());
    }
    let refs: Vec<&[u8]> = parts.iter().map(Vec::as_slice).collect();
    words_of(&hash(b"Zafe_SafetyWords", &refs))
}

fn words_of(h: &[u8; 32]) -> String {
    let bits = u64::from_be_bytes(h[..8].try_into().expect("8 bytes"));
    (0..WORDS)
        .map(|i| English::word_of(((bits >> (64 - 11 * (i + 1))) & 0x7ff) as usize))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn recovery_commitment(new: &IdentityPublic, secret: &[u8; 32]) -> [u8; 32] {
    hash(b"Zafe_RecovCommit", &[&new.fingerprint(), secret])
}

pub fn recovery_words(
    new: &IdentityPublic,
    cosigner: &[u8; 32],
    secret: &[u8; 32],
    challenge: &[u8; 32],
) -> String {
    words_of(&hash(
        b"Zafe_RecovWords_",
        &[&new.fingerprint(), cosigner, secret, challenge],
    ))
}

pub fn normalize(words: &str) -> String {
    words
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use rand::{rngs::StdRng, RngCore, SeedableRng};
    use zafe_proto::Identity;

    use super::*;

    #[test]
    fn words_cover_every_member_and_value() {
        let mut rng = StdRng::seed_from_u64(1);
        let ids: Vec<Identity> = (0..3).map(|_| Identity::generate(&mut rng)).collect();
        let mailbox = [7; 16];
        let set: Vec<_> = ids
            .iter()
            .map(|i| {
                let mut value = [0; 32];
                rng.fill_bytes(&mut value);
                (*i.public(), value)
            })
            .collect();
        let w = words(&mailbox, &set);
        assert_eq!(w.split(' ').count(), WORDS);
        let mut reversed = set.clone();
        reversed.reverse();
        assert_eq!(w, words(&mailbox, &reversed), "order-independent");
        let mut other_value = set.clone();
        other_value[1].1[0] ^= 1;
        assert_ne!(w, words(&mailbox, &other_value));
        assert_ne!(w, words(&[0; 16], &set), "bound to the vault");
        assert_eq!(
            normalize("  Apple  BANANA\tcat dog "),
            "apple banana cat dog"
        );
    }
}
