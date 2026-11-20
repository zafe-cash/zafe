use rand::{rngs::StdRng, RngCore, SeedableRng};
use zafe_core::pq::{PqError, PqKeys, PqPublic, CIPHERTEXT_LEN, PUBLIC_KEY_LEN};
use zafe_proto::identity::IdentitySeeds;

fn seeds(rng: &mut StdRng) -> IdentitySeeds {
    let mut s = IdentitySeeds {
        sig_seed: [0; 32],
        enc_seed: [0; 32],
    };
    rng.fill_bytes(&mut s.sig_seed);
    rng.fill_bytes(&mut s.enc_seed);
    s
}

#[test]
fn sealed_payload_opens_only_for_its_key_and_context() {
    let mut rng = StdRng::seed_from_u64(1);
    let alice = PqKeys::derive(&seeds(&mut rng));
    let bob = PqKeys::derive(&seeds(&mut rng));
    let secret = [7u8; 32];

    let sealed = alice.public().seal(b"ctx", &secret, &mut rng);
    assert_eq!(sealed.len(), CIPHERTEXT_LEN + 24 + 32 + 16);
    assert_eq!(alice.open(b"ctx", &sealed).unwrap(), secret);

    assert_eq!(alice.open(b"other ctx", &sealed), Err(PqError::Open));
    assert_eq!(bob.open(b"ctx", &sealed), Err(PqError::Open));
    for at in [0, CIPHERTEXT_LEN, CIPHERTEXT_LEN + 24, sealed.len() - 1] {
        let mut tampered = sealed.clone();
        tampered[at] ^= 1;
        assert_eq!(
            alice.open(b"ctx", &tampered),
            Err(PqError::Open),
            "byte {at}"
        );
    }
    assert_eq!(alice.open(b"ctx", &sealed[..100]), Err(PqError::Open));
}

#[test]
fn keys_derive_from_identity_seeds() {
    let mut rng = StdRng::seed_from_u64(2);
    let s = seeds(&mut rng);
    let again = IdentitySeeds {
        sig_seed: s.sig_seed,
        enc_seed: s.enc_seed,
    };
    assert_eq!(PqKeys::derive(&s).public(), PqKeys::derive(&again).public());

    let mut other = IdentitySeeds {
        sig_seed: s.sig_seed,
        enc_seed: s.enc_seed,
    };
    other.enc_seed[0] ^= 1;
    assert_ne!(PqKeys::derive(&s).public(), PqKeys::derive(&other).public());
}

#[test]
fn public_keys_round_trip_and_malformed_ones_are_refused() {
    let mut rng = StdRng::seed_from_u64(3);
    let keys = PqKeys::derive(&seeds(&mut rng));
    let bytes = keys.public().to_bytes();
    assert_eq!(bytes.len(), PUBLIC_KEY_LEN);
    assert_eq!(&PqPublic::from_bytes(&bytes).unwrap(), keys.public());

    assert_eq!(
        PqPublic::from_bytes(&bytes[..PUBLIC_KEY_LEN - 1]),
        Err(PqError::BadPublicKey)
    );
    assert_eq!(
        PqPublic::from_bytes(&[0xff; PUBLIC_KEY_LEN]),
        Err(PqError::BadPublicKey),
        "coefficients above the modulus fail the FIPS 203 check"
    );
}
