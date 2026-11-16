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

pub type MailboxId = [u8; 16];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Recipient {
    One([u8; 32]),
    All,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Join,
    DkgRound1,
    DkgEcho,
    DkgRound2,
    SkContribution,
    DescriptorSignature,
    LogKey,
    LogEntry,
    Approval,
    SigningRequest,
    SignatureShares,
    RepairDelta,
    RepairSigma,
    SafetyCommit,
    SafetyReveal,
    MoveNotice,
    Arrival,
    MoveCancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub version: u16,
    pub mailbox: MailboxId,
    pub from: [u8; 32],
    pub to: Recipient,
    pub seq: u64,
    pub kind: Kind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    pub header: Header,
    pub body: Vec<u8>,
    pub signature: Vec<u8>,
}

impl Envelope {
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

    pub fn verify(&self, sender: &IdentityPublic) -> Result<(), ProtoError> {
        version::check(Format::Envelope, self.header.version)?;
        if self.header.from != sender.sig_pk {
            return Err(ProtoError::WrongSender);
        }
        sender.verify(&signed_bytes(&self.header, &self.body)?, &self.signature)
    }

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

    pub fn to_bytes(&self) -> Result<Vec<u8>, ProtoError> {
        Ok(version::frame(Format::Envelope, &encode(self)?))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ProtoError> {
        let envelope: Self = postcard::from_bytes(version::unframe(Format::Envelope, bytes)?)
            .map_err(|_| ProtoError::Encoding)?;
        if envelope.header.version != version::ENVELOPE {
            return Err(ProtoError::Encoding);
        }
        Ok(envelope)
    }
}

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
