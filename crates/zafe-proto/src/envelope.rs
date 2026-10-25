//! Signed envelopes, optionally HPKE-sealed to one recipient (spec §5.2).
//!
//! The header is signed and, for sealed envelopes, is the HPKE AAD, so a ciphertext cannot
//! be moved into another envelope. Recipients reject envelopes whose signature fails, whose
//! sender is not a current member, or whose sequence number was already seen.

use std::collections::BTreeMap;

use hpke::{
    aead::ChaCha20Poly1305, kdf::HkdfSha256, Deserializable, OpModeR, OpModeS, Serializable,
};
use rand_core::{CryptoRng, RngCore};
use serde::{Deserialize, Serialize};

use crate::{
    identity::{Identity, IdentityPublic, Kem},
    version::{self, Format},
    ProtoError,
};

const SIGNATURE_DOMAIN: &[u8] = b"Zafe envelope signature v1";
const HPKE_INFO: &[u8] = b"Zafe HPKE v1";

/// Opaque relay mailbox id for a vault (or a vault being created).
pub type MailboxId = [u8; 16];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Recipient {
    /// Sealed to one member, identified by their Ed25519 key.
    One([u8; 32]),
    /// Public to every member of the mailbox (e.g. DKG round-1 packages, echo hashes).
    All,
}

/// Application-level message kinds. The payload encoding is kind-specific.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Join,
    DkgRound1,
    DkgEcho,
    DkgRound2,
    SkContribution,
    DescriptorSignature,
    /// The vault log key, sealed to one member.
    LogKey,
    LogEntry,
    Approval,
    SigningRequest,
    SignatureShares,
    /// Share repair, helper to helper: one RTS delta (spec §10.4.2).
    RepairDelta,
    /// Share repair, helper to the member being repaired: an RTS sigma plus the vault
    /// secret, log key and public key package.
    RepairSigma,
    /// Setup safety words, step 1: a member's commitment to its secret value (public).
    SafetyCommit,
    /// Setup safety words, step 2: the value itself, sent once every commitment is in.
    SafetyReveal,
    /// A relay move, on the vault's current relay: where the vault is moving, with the
    /// mover's arrival; later also the `VaultMoved` entry (sealed, never acknowledged).
    MoveNotice,
    /// A relay move, on the new relay: a member's signed arrival (sealed per recipient).
    Arrival,
    /// A relay move: the sender withdraws its arrival (the mover: the whole move).
    MoveCancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    /// [`version::ENVELOPE`]; signed, and part of the HPKE associated data.
    pub version: u16,
    pub mailbox: MailboxId,
    /// Sender's Ed25519 key.
    pub from: [u8; 32],
    pub to: Recipient,
    /// Per-sender, strictly increasing within a mailbox.
    pub seq: u64,
    pub kind: Kind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    pub header: Header,
    /// Plaintext for `Recipient::All`; `enc || ciphertext` for `Recipient::One`.
    pub body: Vec<u8>,
    /// Ed25519 signature over the domain, header and body.
    pub signature: Vec<u8>,
}

impl Envelope {
    /// A signed, unencrypted envelope to every member.
    pub fn public(
        sender: &Identity,
        mailbox: MailboxId,
        seq: u64,
        kind: Kind,
        payload: &[u8],
    ) -> Result<Self, ProtoError> {
        let header = Header {
            version: version::ENVELOPE,
            mailbox,
            from: sender.public().sig_pk,
            to: Recipient::All,
            seq,
            kind,
        };
        Self::sign(sender, header, payload.to_vec())
    }

    /// A signed envelope HPKE-sealed to `recipient`.
    pub fn sealed<R: RngCore + CryptoRng>(
        sender: &Identity,
        recipient: &IdentityPublic,
        mailbox: MailboxId,
        seq: u64,
        kind: Kind,
        payload: &[u8],
        rng: &mut R,
    ) -> Result<Self, ProtoError> {
        let header = Header {
            version: version::ENVELOPE,
            mailbox,
            from: sender.public().sig_pk,
            to: Recipient::One(recipient.sig_pk),
            seq,
            kind,
        };
        let aad = encode(&header)?;
        let (encapped, ciphertext) =
            hpke::single_shot_seal::<ChaCha20Poly1305, HkdfSha256, Kem, _>(
                &OpModeS::Base,
                &recipient.hpke_public()?,
                HPKE_INFO,
                payload,
                &aad,
                rng,
            )
            .map_err(|_| ProtoError::Crypto)?;
        let mut body = encapped.to_bytes().to_vec();
        body.extend_from_slice(&ciphertext);
        Self::sign(sender, header, body)
    }

    /// Signs an arbitrary header and body. Low-level: callers normally use [`Self::public`]
    /// or [`Self::sealed`]. Exposed to test that sealed payloads are bound to their header.
    #[doc(hidden)]
    pub fn signed_raw(
        sender: &Identity,
        header: Header,
        body: Vec<u8>,
    ) -> Result<Self, ProtoError> {
        Self::sign(sender, header, body)
    }

    fn sign(sender: &Identity, header: Header, body: Vec<u8>) -> Result<Self, ProtoError> {
        let signature = sender.sign(&signed_bytes(&header, &body)?).to_vec();
        Ok(Self {
            header,
            body,
            signature,
        })
    }

    /// Checks the version and the sender's signature. The relay and recipients both do this.
    pub fn verify(&self, sender: &IdentityPublic) -> Result<(), ProtoError> {
        version::check(Format::Envelope, self.header.version)?;
        if self.header.from != sender.sig_pk {
            return Err(ProtoError::WrongSender);
        }
        sender.verify(&signed_bytes(&self.header, &self.body)?, &self.signature)
    }

    /// Verifies the envelope and returns its payload, decrypting it if it is sealed to `me`.
    pub fn open(&self, me: &Identity, sender: &IdentityPublic) -> Result<Vec<u8>, ProtoError> {
        self.verify(sender)?;
        match self.header.to {
            Recipient::All => Ok(self.body.clone()),
            Recipient::One(to) if to == me.public().sig_pk => {
                if self.body.len() < 32 {
                    return Err(ProtoError::Crypto);
                }
                let (enc, ciphertext) = self.body.split_at(32);
                let encapped = <Kem as hpke::Kem>::EncappedKey::from_bytes(enc)
                    .map_err(|_| ProtoError::Crypto)?;
                hpke::single_shot_open::<ChaCha20Poly1305, HkdfSha256, Kem>(
                    &OpModeR::Base,
                    me.hpke_private(),
                    &encapped,
                    HPKE_INFO,
                    ciphertext,
                    &encode(&self.header)?,
                )
                .map_err(|_| ProtoError::Crypto)
            }
            Recipient::One(_) => Err(ProtoError::NotForMe),
        }
    }

    /// `version (u16) || postcard(envelope)`; the tag equals `header.version`.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ProtoError> {
        Ok(version::frame(Format::Envelope, &encode(self)?))
    }

    /// Rejects unknown versions with [`ProtoError::UnsupportedVersion`] before parsing.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ProtoError> {
        let envelope: Self = postcard::from_bytes(version::unframe(Format::Envelope, bytes)?)
            .map_err(|_| ProtoError::Encoding)?;
        if envelope.header.version != version::ENVELOPE {
            return Err(ProtoError::Encoding);
        }
        Ok(envelope)
    }
}

/// Rejects replayed or reordered envelopes: sequence numbers must strictly increase per
/// (mailbox, sender).
#[derive(Default, Debug)]
pub struct ReplayGuard(BTreeMap<(MailboxId, [u8; 32]), u64>);

impl ReplayGuard {
    pub fn check_and_record(&mut self, header: &Header) -> Result<(), ProtoError> {
        let key = (header.mailbox, header.from);
        match self.0.get(&key) {
            Some(last) if header.seq <= *last => Err(ProtoError::Replay {
                seq: header.seq,
                last: *last,
            }),
            _ => {
                self.0.insert(key, header.seq);
                Ok(())
            }
        }
    }
}

fn signed_bytes(header: &Header, body: &[u8]) -> Result<Vec<u8>, ProtoError> {
    let mut out = SIGNATURE_DOMAIN.to_vec();
    let h = encode(header)?;
    out.extend_from_slice(&(h.len() as u32).to_le_bytes());
    out.extend_from_slice(&h);
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(body);
    Ok(out)
}

pub(crate) fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, ProtoError> {
    postcard::to_allocvec(value).map_err(|_| ProtoError::Encoding)
}
