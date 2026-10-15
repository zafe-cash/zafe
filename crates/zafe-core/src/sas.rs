//! Setup safety words (spec §5.3): a commit-then-reveal short authentication string.
//!
//! Members compare a few words out of band before key generation, to catch a relay that
//! shows members different member lists (inserting identities of its own). A plain hash
//! of the member keys, as before, could be ground: the relay can try key after key until
//! two views hash alike (12 digits is about 40 bits, days on one core). Here every member
//! also contributes a secret value, commits to it, and reveals it only after it has every
//! other member's commitment. The words cover all members' keys and values, so whatever
//! the relay commits to in one view, the honest member's value in the other is fixed
//! after it and unknown to it: matching words are a one-shot guess (2^-44 for 4 words).
//!
//! The value is a deterministic signature-derived secret, so a restarted app reveals the
//! same value it committed to, with nothing new to store.

use bip0039::{English, Language};
use zafe_proto::{Identity, IdentityPublic, MailboxId};

/// Words members compare; 11 bits each (BIP-39 English list).
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

/// This member's secret value for the setup of `mailbox`. Ed25519 signatures are
/// deterministic, so it is the same on every call and unknown to anyone else.
pub fn secret(me: &Identity, mailbox: &MailboxId) -> [u8; 32] {
    let mut message = b"Zafe setup safety words v1".to_vec();
    message.extend_from_slice(mailbox);
    hash(b"Zafe_SasSecret__", &[&me.sign(&message)])
}

/// The commitment a member publishes first.
pub fn commitment(mailbox: &MailboxId, sig_pk: &[u8; 32], secret: &[u8; 32]) -> [u8; 32] {
    hash(b"Zafe_SasCommit__", &[mailbox, sig_pk, secret])
}

/// The words, from every member's identity and revealed value (any order).
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

/// [`WORDS`] BIP-39 words from the first 44 bits of `h`.
fn words_of(h: &[u8; 32]) -> String {
    let bits = u64::from_be_bytes(h[..8].try_into().expect("8 bytes"));
    (0..WORDS)
        .map(|i| English::word_of(((bits >> (64 - 11 * (i + 1))) & 0x7ff) as usize))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Seat recovery (spec §10.1): the commitment a new phone publishes to its secret value
/// before any co-signer challenges it.
pub fn recovery_commitment(new: &IdentityPublic, secret: &[u8; 32]) -> [u8; 32] {
    hash(b"Zafe_RecovCommit", &[&new.fingerprint(), secret])
}

/// The words a new phone and one co-signer compare before the co-signer approves moving
/// a seat to it: over the new identity, the co-signer's key, the new phone's revealed
/// secret and the co-signer's challenge. A recovery code swapped on the way commits to its
/// value before the challenge exists, so a match is a one-shot guess (2^-44).
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

/// The same words, however a person typed them (case, spacing).
pub fn normalize(words: &str) -> String {
    words
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use rand::{rngs::StdRng, SeedableRng};

    use super::*;

    #[test]
    fn words_cover_every_member_and_value() {
        let mut rng = StdRng::seed_from_u64(1);
        let ids: Vec<Identity> = (0..3).map(|_| Identity::generate(&mut rng)).collect();
        let mailbox = [7; 16];
        let set: Vec<_> = ids
            .iter()
            .map(|i| (*i.public(), secret(i, &mailbox)))
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
            secret(&ids[0], &mailbox),
            secret(&ids[0], &mailbox),
            "stable"
        );
        assert_ne!(secret(&ids[0], &mailbox), secret(&ids[0], &[8; 16]));
        assert_eq!(
            normalize("  Apple  BANANA\tcat dog "),
            "apple banana cat dog"
        );
    }
}
