//! Vault-log replay rules (spec §6.3, §9.6).

use rand::{rngs::StdRng, SeedableRng};
use zafe_core::node;
use zafe_core::vault::{
    MemberInfo, ProposalStatus, ProposedPayment, VaultDescriptor, VaultError, VaultEvent,
    VaultState,
};
use zafe_proto::{
    version::{self, Format, UnsupportedVersion},
    Chain, Identity, LogEntry, LogKey,
};

mod common;

const MAILBOX: [u8; 16] = [4; 16];

struct Log {
    rng: StdRng,
    ids: Vec<Identity>,
    key: LogKey,
    chain: Chain,
}

impl Log {
    fn new() -> Self {
        let mut rng = StdRng::seed_from_u64(8);
        let ids = (0..4).map(|_| Identity::generate(&mut rng)).collect();
        let key = LogKey::generate(0, &mut rng);
        Self {
            rng,
            ids,
            key,
            chain: Chain::new(MAILBOX),
        }
    }

    fn descriptor(&self) -> VaultDescriptor {
        VaultDescriptor {
            vault_id: MAILBOX,
            version: zafe_proto::version::DESCRIPTOR,
            name: "Grants".into(),
            network: "regtest".into(),
            threshold: 2,
            members: self.ids[..3]
                .iter()
                .enumerate()
                .map(|(i, id)| MemberInfo {
                    identity: *id.public(),
                    frost_id: vec![i as u8],
                    name: format!("m{i}"),
                })
                .collect(),
            group_public_key: [1; 32],
            ufvk: "uviewregtest1...".into(),
            address: "uregtest1...".into(),
            use_qsk: true,
            birthday_height: 2,
            proposal_expiry_blocks: 8064,
            epoch: 0,
            transcript_hash: [2; 32],
        }
    }

    fn created(&self, signers: &[usize]) -> VaultEvent {
        let d = self.descriptor();
        let msg = d.signing_message().unwrap();
        let signatures = signers
            .iter()
            .map(|i| {
                (
                    self.ids[*i].public().sig_pk,
                    self.ids[*i].sign(&msg).to_vec(),
                )
            })
            .collect();
        VaultEvent::Created {
            descriptor: d,
            signatures,
        }
    }

    /// Appends an event authored by member `author` (the relay only accepts members, but
    /// replay must not rely on that, so the chain here is checked against all 4 ids).
    fn push(&mut self, author: usize, event: &VaultEvent) {
        self.push_raw(author, &event.to_bytes().unwrap());
    }

    /// Appends an entry whose plaintext is `bytes` as is.
    fn push_raw(&mut self, author: usize, bytes: &[u8]) {
        let entry = LogEntry::create(
            &self.ids[author],
            &self.key,
            MAILBOX,
            self.chain.len(),
            self.chain.head(),
            bytes,
            &mut self.rng,
        )
        .unwrap();
        let all: Vec<_> = self.ids.iter().map(|i| *i.public()).collect();
        self.chain.append(entry, &all).unwrap();
    }

    fn replay(&self) -> Result<VaultState, VaultError> {
        VaultState::replay(self.chain.entries(), &self.key)
    }
}

fn proposal(id: u8) -> VaultEvent {
    VaultEvent::Proposal {
        id: [id; 16],
        payments: vec![ProposedPayment {
            address: "uregtest1payee".into(),
            amount_zat: 1,
            memo: vec![0xF6],
        }],
        pczt: vec![1, 2, 3],
        pczt_hash: [id; 32],
        tip_height: 100,
        created_at: 1_700_000_000,
        signing_spends: 1,
        auto_send: false,
    }
}

fn vote(id: u8, approve: bool) -> VaultEvent {
    VaultEvent::Vote {
        proposal: [id; 16],
        pczt_hash: [id; 32],
        approve,
        commitments: vec![],
        shares: vec![],
    }
}

#[test]
fn proposal_lifecycle() {
    let mut log = Log::new();
    log.push(0, &log.created(&[0, 1, 2]));
    log.push(0, &proposal(1));
    log.push(1, &vote(1, true));
    assert_eq!(
        log.replay().unwrap().proposals[&[1; 16]].status,
        ProposalStatus::Open
    );
    log.push(2, &vote(1, true));
    assert_eq!(
        log.replay().unwrap().proposals[&[1; 16]].status,
        ProposalStatus::Approved
    );
    log.push(
        0,
        &VaultEvent::Broadcast {
            proposal: [1; 16],
            txid: [9; 32],
        },
    );
    let state = log.replay().unwrap();
    assert_eq!(state.proposals[&[1; 16]].status, ProposalStatus::Broadcast);
    assert_eq!(state.applied, 5);
}

#[test]
fn rejection_threshold_is_n_minus_t_plus_one() {
    let mut log = Log::new();
    log.push(0, &log.created(&[0, 1, 2]));
    log.push(0, &proposal(1));
    log.push(1, &vote(1, false));
    assert_eq!(
        log.replay().unwrap().proposals[&[1; 16]].status,
        ProposalStatus::Open,
        "1 reject of 3 is not enough"
    );
    log.push(2, &vote(1, false));
    assert_eq!(
        log.replay().unwrap().proposals[&[1; 16]].status,
        ProposalStatus::Rejected
    );
}

#[test]
fn a_member_changing_their_vote_counts_once() {
    let mut log = Log::new();
    log.push(0, &log.created(&[0, 1, 2]));
    log.push(0, &proposal(1));
    log.push(1, &vote(1, true));
    log.push(1, &vote(1, true)); // re-approval with fresh commitments
    assert_eq!(log.replay().unwrap().proposals[&[1; 16]].approvals.len(), 1);
    log.push(1, &vote(1, false));
    let p = &log.replay().unwrap().proposals[&[1; 16]];
    assert!(p.approvals.is_empty());
    assert_eq!(p.rejections.len(), 1);
}

#[test]
fn creation_needs_every_member_signature() {
    let mut log = Log::new();
    log.push(0, &log.created(&[0, 1]));
    assert_eq!(
        log.replay().unwrap_err(),
        VaultError::MissingDescriptorSignature
    );
}

#[test]
fn log_must_start_with_creation() {
    let mut log = Log::new();
    log.push(0, &proposal(1));
    assert_eq!(log.replay().unwrap_err(), VaultError::NotCreatedFirst);
}

/// Invalid entries are skipped by every member instead of making the log unreadable.
#[test]
fn invalid_entries_are_ignored_not_fatal() {
    let cases: Vec<(usize, VaultEvent, VaultError)> = vec![
        (3, proposal(2), VaultError::NotAMember(2)), // id 3 is not in the descriptor
        (
            1,
            VaultEvent::Vote {
                proposal: [1; 16],
                pczt_hash: [0xEE; 32],
                approve: true,
                commitments: vec![],
                shares: vec![],
            },
            VaultError::PcztMismatch(2),
        ),
        (
            1,
            VaultEvent::Cancelled { proposal: [1; 16] },
            VaultError::NotAuthor(2),
        ),
        (
            0,
            VaultEvent::Broadcast {
                proposal: [1; 16],
                txid: [0; 32],
            },
            VaultError::ProposalClosed(2),
        ),
        (1, vote(9, true), VaultError::UnknownProposal(2)),
    ];
    for (author, bad, expected) in cases {
        let mut log = Log::new();
        log.push(0, &log.created(&[0, 1, 2]));
        log.push(0, &proposal(1));
        log.push(author, &bad);
        log.push(1, &vote(1, true)); // entries after the bad one still apply
        let state = log.replay().expect("log stays readable");
        assert_eq!(state.ignored, vec![(2, expected)]);
        assert_eq!(state.proposals.len(), 1);
        assert_eq!(state.proposals[&[1; 16]].approvals.len(), 1);
        assert_eq!(state.applied, 4);
    }
}

/// An event from a newer version of Zafe after creation is skipped deterministically (and
/// counted, so the app can ask to update); the same at creation is fatal.
#[test]
fn unknown_event_versions_are_ignored_after_creation() {
    let newer = |event: &VaultEvent| {
        let mut bytes = event.to_bytes().unwrap();
        assert_eq!(&bytes[..2], &version::VAULT_EVENT.to_le_bytes());
        bytes[..2].copy_from_slice(&(version::VAULT_EVENT + 1).to_le_bytes());
        bytes
    };
    let unsupported = UnsupportedVersion {
        format: Format::VaultEvent,
        found: version::VAULT_EVENT + 1,
        supported: version::VAULT_EVENT,
    };

    let mut log = Log::new();
    log.push(0, &log.created(&[0, 1, 2]));
    log.push(0, &proposal(1));
    log.push_raw(1, &newer(&vote(1, true)));
    log.push(2, &vote(1, true));
    let state = log.replay().expect("log stays readable");
    assert_eq!(
        state.ignored,
        vec![(2, VaultError::UnsupportedVersion(unsupported))]
    );
    assert_eq!(state.newer_version_entries(), 1);
    assert_eq!(state.proposals[&[1; 16]].approvals.len(), 1);
    assert_eq!(state.applied, 4);

    let mut log = Log::new();
    let created = log.created(&[0, 1, 2]);
    log.push_raw(0, &newer(&created));
    assert_eq!(
        log.replay().unwrap_err(),
        VaultError::UnsupportedVersion(unsupported)
    );
}

fn name(name: &str) -> VaultEvent {
    VaultEvent::Name { name: name.into() }
}

/// Members name themselves; the latest name per author wins, empty clears it, and an
/// invalid name is ignored like any other invalid entry.
#[test]
fn members_share_their_names() {
    let mut log = Log::new();
    log.push(0, &log.created(&[0, 1, 2]));
    log.push(0, &name("Alice"));
    log.push(1, &name("Bob"));
    log.push(0, &name("Alice K."));
    log.push(1, &name(""));
    log.push(2, &name(" padded "));
    log.push(2, &name(&"x".repeat(33)));
    log.push(2, &name("tab\there"));
    log.push(3, &name("Mallory")); // not a member
    let state = log.replay().unwrap();
    let pk: Vec<_> = log.ids.iter().map(|i| i.public().sig_pk).collect();
    assert_eq!(state.names.len(), 1);
    assert_eq!(state.names[&pk[0]], "Alice K.");
    assert_eq!(
        state.ignored,
        vec![
            (5, VaultError::BadName(5)),
            (6, VaultError::BadName(6)),
            (7, VaultError::BadName(7)),
            (8, VaultError::NotAMember(8)),
        ]
    );
    // 32 characters (not bytes) is the limit.
    log.push(2, &name(&"桜".repeat(32)));
    assert_eq!(log.replay().unwrap().names[&pk[2]], "桜".repeat(32));
}

/// Members attest their backups; the latest attestation per member wins, one for another
/// key epoch is ignored, and non-members can't attest.
#[test]
fn members_attest_backups() {
    let mut log = Log::new();
    log.push(0, &log.created(&[0, 1, 2]));
    let attest = |epoch, at| VaultEvent::BackupVerified { epoch, at };
    log.push(0, &attest(0, 100));
    log.push(1, &attest(0, 200));
    log.push(0, &attest(0, 300));
    log.push(2, &attest(1, 400)); // a future epoch
    log.push(3, &attest(0, 500)); // not a member
    let state = log.replay().unwrap();
    let pk: Vec<_> = log.ids.iter().map(|i| i.public().sig_pk).collect();
    assert_eq!(state.backups.len(), 2);
    assert_eq!(state.backups[&pk[0]].at, 300);
    assert_eq!(state.backups[&pk[1]].at, 200);
    assert_eq!(
        state.ignored,
        vec![
            (4, VaultError::BadBackup(4)),
            (5, VaultError::NotAMember(5))
        ]
    );
}

impl Log {
    /// Member `author` approves moving `old`'s seat to `ids[new]`.
    fn replace(&mut self, author: usize, old: usize, new: usize) {
        let approval = zafe_proto::relay::ReplaceApproval {
            mailbox: MAILBOX,
            old: self.ids[old].public().sig_pk,
            new: *self.ids[new].public(),
        };
        let event = VaultEvent::ReplaceApproval {
            old: approval.old,
            new: approval.new,
            signature: approval.sign(&self.ids[author]).unwrap(),
        };
        self.push(author, &event);
    }
}

/// A seat moves at the threshold of approvals from other members; the seat keeps its
/// votes, name and FROST identifier under the new key, the old key is out, and invalid
/// approvals are ignored.
#[test]
fn a_seat_moves_at_the_threshold() {
    let mut log = Log::new();
    log.push(0, &log.created(&[0, 1, 2]));
    log.push(0, &name("Alice"));
    log.push(0, &proposal(1));
    log.push(0, &vote(1, true)); // one-tap style: no commitments
    log.push(0, &VaultEvent::BackupVerified { epoch: 0, at: 1 });
    let pk: Vec<_> = log.ids.iter().map(|i| i.public().sig_pk).collect();

    log.replace(0, 0, 3); // the lost member can't approve its own move
    log.replace(1, 1, 1); // nor can a member move to a key that is already a member
    let mut forged = VaultEvent::ReplaceApproval {
        old: pk[0],
        new: *log.ids[3].public(),
        signature: vec![0; 64],
    };
    log.push(1, &forged);
    log.replace(1, 0, 3);
    log.replace(1, 0, 3); // twice from the same member counts once
    let state = log.replay().unwrap();
    assert!(state.replacements.is_empty());
    assert_eq!(state.pending_replacements.len(), 1);
    assert_eq!(
        state.ignored.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        vec![5, 6, 7, 9]
    );

    log.replace(2, 0, 3);
    let state = log.replay().unwrap();
    let moved = &state.replacements[0];
    assert_eq!((moved.old, moved.new.sig_pk), (pk[0], pk[3]));
    assert_eq!(moved.helpers(), vec![pk[1], pk[2]]);
    assert_eq!(moved.frost_id, vec![0]);
    assert!(state.pending_replacements.is_empty());
    assert_eq!(state.descriptor.member(&pk[3]).unwrap().frost_id, vec![0]);
    assert!(state.descriptor.member(&pk[0]).is_none());
    assert_eq!(state.names[&pk[3]], "Alice");
    assert!(state.backups.is_empty(), "the new device has no backup yet");
    let p = &state.proposals[&[1; 16]];
    assert_eq!(p.author, pk[3]);
    assert!(p.approvals.contains_key(&pk[3]));

    // The old key is out; the new one is a member.
    log.push(0, &name("Mallory"));
    log.push(3, &name("Alice 2"));
    let state = log.replay().unwrap();
    assert_eq!(
        state.ignored.last(),
        Some(&(11, VaultError::NotAMember(11)))
    );
    assert_eq!(state.names[&pk[3]], "Alice 2");

    // An approval that arrives after the move is moot.
    forged = VaultEvent::Name { name: "x".into() };
    log.push(3, &forged);
    log.replace(1, 0, 3);
    assert!(matches!(
        log.replay().unwrap().ignored.last(),
        Some((14, VaultError::BadReplacement(14)))
    ));
}

/// After a seat moves, any member can restart its key repair with other helpers (the
/// threshold of current members, itself among them, never the new key), and only the new
/// key can mark it done; nothing changes a finished repair.
#[test]
fn a_stalled_repair_restarts_and_finishes() {
    let mut log = Log::new();
    log.push(0, &log.created(&[0, 1, 2]));
    log.replace(1, 0, 3);
    log.replace(2, 0, 3);
    let pk: Vec<_> = log.ids.iter().map(|i| i.public().sig_pk).collect();
    let state = log.replay().unwrap();
    let index = state.replacements[0].index;
    assert_eq!(state.replacements[0].helpers, vec![pk[1], pk[2]]);
    let retry = |helpers: Vec<[u8; 32]>| VaultEvent::RepairRetry {
        replacement: index,
        helpers,
    };
    let done = VaultEvent::RepairDone { replacement: index };

    log.push(3, &retry(vec![pk[3], pk[1]])); // 3: the new key can't help itself
    log.push(1, &retry(vec![pk[1], pk[0]])); // 4: the old key isn't a member
    log.push(1, &retry(vec![pk[1]])); // 5: one helper short
    log.push(1, &retry(vec![pk[2], pk[3]])); // 6: the author must help
    log.push(1, &retry(vec![pk[1], pk[1]])); // 7: twice the same helper
    log.push(2, &retry(vec![pk[2], pk[1]])); // 8: valid
    log.push(1, &done); // 9: only the new key says it's done
    let state = log.replay().unwrap();
    let r = &state.replacements[0];
    assert_eq!(
        (r.attempt, r.helpers.clone(), r.done),
        (1, vec![pk[2], pk[1]], false)
    );
    assert_eq!(
        state.ignored.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        vec![3, 4, 5, 6, 7, 9]
    );

    log.push(3, &done);
    log.push(2, &retry(vec![pk[2], pk[1]])); // 11: too late
    log.push(3, &done); // 12: once
    let state = log.replay().unwrap();
    assert!(state.replacements[0].done);
    assert_eq!(state.replacements[0].attempt, 1);
    assert_eq!(
        state
            .ignored
            .iter()
            .map(|(i, _)| *i)
            .rev()
            .take(2)
            .collect::<Vec<_>>(),
        vec![12, 11]
    );
}

/// Interactive approvals of the lost device carry commitments whose nonces were on it:
/// the move voids them, so a proposal approved at exactly the threshold reopens.
#[test]
fn a_seat_move_voids_interactive_approvals() {
    let mut log = Log::new();
    log.push(0, &log.created(&[0, 1, 2]));
    log.push(0, &proposal(1));
    let interactive = |id: u8| VaultEvent::Vote {
        proposal: [id; 16],
        pczt_hash: [id; 32],
        approve: true,
        commitments: vec![vec![1; 64]],
        shares: vec![],
    };
    log.push(0, &interactive(1));
    log.push(1, &interactive(1));
    assert_eq!(
        log.replay().unwrap().proposals[&[1; 16]].status,
        ProposalStatus::Approved
    );
    log.replace(1, 0, 3);
    log.replace(2, 0, 3);
    let state = log.replay().unwrap();
    let p = &state.proposals[&[1; 16]];
    assert_eq!(p.status, ProposalStatus::Open);
    assert_eq!(p.approvals.len(), 1);
    // The new device approves again.
    log.push(3, &interactive(1));
    assert_eq!(
        log.replay().unwrap().proposals[&[1; 16]].status,
        ProposalStatus::Approved
    );
}

/// Version 2 only appended `Name`: a log written by version 1 still replays the same.
#[test]
fn version_1_events_still_replay() {
    let v1 = |event: &VaultEvent| {
        let mut bytes = event.to_bytes().unwrap();
        bytes[..2].copy_from_slice(&1u16.to_le_bytes());
        bytes
    };
    let mut log = Log::new();
    let created = log.created(&[0, 1, 2]);
    log.push_raw(0, &v1(&created));
    log.push_raw(0, &v1(&proposal(1)));
    log.push_raw(1, &v1(&vote(1, true)));
    log.push(2, &vote(1, true));
    let state = log.replay().expect("version 1 log replays");
    assert!(state.ignored.is_empty());
    assert_eq!(state.proposals[&[1; 16]].status, ProposalStatus::Approved);
    // Version 0 never existed.
    let mut zero = proposal(2).to_bytes().unwrap();
    zero[..2].copy_from_slice(&0u16.to_le_bytes());
    log.push_raw(0, &zero);
    assert!(matches!(
        log.replay().unwrap().ignored[..],
        [(4, VaultError::UnsupportedVersion(_))]
    ));
}

/// A descriptor from another version is fatal even when every member signed it.
#[test]
fn unknown_descriptor_version_is_fatal() {
    let mut log = Log::new();
    let mut d = log.descriptor();
    d.version = version::DESCRIPTOR + 1;
    let msg = d.signing_message().unwrap();
    let signatures = (0..3)
        .map(|i| (log.ids[i].public().sig_pk, log.ids[i].sign(&msg).to_vec()))
        .collect();
    log.push(
        0,
        &VaultEvent::Created {
            descriptor: d,
            signatures,
        },
    );
    assert!(matches!(
        log.replay().unwrap_err(),
        VaultError::UnsupportedVersion(v) if v.format == Format::Descriptor && v.is_newer()
    ));
}

/// The descriptor's version is covered by the members' signatures.
#[test]
fn descriptor_version_is_signed() {
    let log = Log::new();
    let mut d = log.descriptor();
    let signed = d.signing_message().unwrap();
    d.version += 1;
    assert_ne!(d.signing_message().unwrap(), signed);
}

/// The race from the code review: the author cancels while the leader broadcasts.
#[test]
fn broadcast_racing_a_cancel_does_not_brick_the_log() {
    let mut log = Log::new();
    log.push(0, &log.created(&[0, 1, 2]));
    log.push(0, &proposal(1));
    log.push(1, &vote(1, true));
    log.push(2, &vote(1, true));
    log.push(0, &VaultEvent::Cancelled { proposal: [1; 16] });
    log.push(
        1,
        &VaultEvent::Broadcast {
            proposal: [1; 16],
            txid: [5; 32],
        },
    );
    let state = log.replay().unwrap();
    assert_eq!(state.proposals[&[1; 16]].status, ProposalStatus::Cancelled);
    assert_eq!(state.ignored, vec![(5, VaultError::ProposalClosed(5))]);

    // `check` lets a writer see this before appending.
    let err = state.check(
        log.ids[1].public().sig_pk,
        &VaultEvent::Broadcast {
            proposal: [1; 16],
            txid: [5; 32],
        },
    );
    assert_eq!(err, Err(VaultError::ProposalClosed(6)));
}

// --- One-tap signing: commitment pools and share votes ---------------------------------

fn commitments(rng: &mut StdRng, n: usize) -> VaultEvent {
    use reddsa::frost::redpallas::{keys::SigningShare, round1};
    // Any valid signing share will do: replay only checks that commitments decode.
    let mut scalar = [0u8; 32];
    scalar[0] = 7;
    let share = SigningShare::deserialize(&scalar).unwrap();
    VaultEvent::Commitments {
        batch: (0..n)
            .map(|_| round1::commit(&share, rng).1.serialize().unwrap())
            .collect(),
    }
}

fn share_vote(id: u8, groups: &[u16]) -> VaultEvent {
    VaultEvent::Vote {
        proposal: [id; 16],
        pczt_hash: [id; 32],
        approve: true,
        commitments: vec![],
        shares: groups
            .iter()
            .map(|g| zafe_core::vault::GroupShares {
                group: *g,
                shares: vec![vec![0xAB; 32]],
            })
            .collect(),
    }
}

/// A 2-of-3 vault whose three members each published `per_member` commitments.
fn pooled_log(per_member: usize) -> Log {
    let mut log = Log::new();
    log.push(0, &log.created(&[0, 1, 2]));
    let mut rng = StdRng::seed_from_u64(99);
    for m in 0..3 {
        log.push(m, &commitments(&mut rng, per_member));
    }
    log
}

#[test]
fn proposals_get_disjoint_commitments_in_log_order() {
    let mut log = pooled_log(4);
    log.push(0, &proposal(1));
    log.push(1, &proposal(2));
    let s = log.replay().unwrap();
    let (p1, p2) = (
        s.proposals[&[1; 16]].preprocessed.as_ref().unwrap(),
        s.proposals[&[2; 16]].preprocessed.as_ref().unwrap(),
    );
    // 2-of-3: three groups, each with one commitment per member per spend.
    assert_eq!(p1.subsets.len(), 3);
    let all: Vec<&Vec<u8>> = [p1, p2]
        .iter()
        .flat_map(|p| p.commitments.iter().flatten().flatten())
        .collect();
    let unique: std::collections::BTreeSet<_> = all.iter().collect();
    assert_eq!(all.len(), 12);
    assert_eq!(unique.len(), 12, "a commitment is never assigned twice");
    // Each member used 2 per proposal: 4 published, 0 left.
    for m in 0..3 {
        assert_eq!(s.pools[&log.ids[m].public().sig_pk].available(), 0);
    }
    // Replay is deterministic.
    let again = log.replay().unwrap();
    assert_eq!(
        again.proposals[&[2; 16]].preprocessed,
        s.proposals[&[2; 16]].preprocessed
    );
}

/// A proposal written by an app on `version` (the tag is the author's `VAULT_EVENT`).
fn proposal_v(id: u8, version: u16) -> Vec<u8> {
    let mut bytes = proposal(id).to_bytes().unwrap();
    bytes[..2].copy_from_slice(&version.to_le_bytes());
    bytes
}

#[test]
fn version_5_proposals_keep_the_all_or_nothing_rule() {
    // Each member needs 2 per proposal; one has 1: the whole proposal is interactive.
    let mut log = pooled_log(1);
    log.push_raw(0, &proposal_v(1, 5));
    let s = log.replay().unwrap();
    assert!(s.proposals[&[1; 16]].preprocessed.is_none());
    for m in 0..3 {
        assert_eq!(s.pools[&log.ids[m].public().sig_pk].available(), 1);
    }
    // A member with no pool at all: the same, for every group.
    let mut log = Log::new();
    log.push(0, &log.created(&[0, 1, 2]));
    let mut rng = StdRng::seed_from_u64(5);
    for m in 0..2 {
        log.push(m, &commitments(&mut rng, 4));
    }
    log.push_raw(0, &proposal_v(1, 5));
    let s = log.replay().unwrap();
    assert!(s.proposals[&[1; 16]].preprocessed.is_none());
    assert_eq!(s.pools[&log.ids[0].public().sig_pk].available(), 4);
}

#[test]
fn short_pools_assign_only_the_groups_they_cover() {
    let mut log = pooled_log(1); // each member needs 2 for all groups, has 1
    log.push(0, &proposal(1));
    let s = log.replay().unwrap();
    let pre = s.proposals[&[1; 16]].preprocessed.as_ref().unwrap();
    // Group order {0,1}, {0,2}, {1,2}: the first takes m0's and m1's only commitment.
    assert_eq!(pre.subsets.len(), 1);
    assert_eq!(
        pre.subsets[0],
        vec![log.ids[0].public().sig_pk, log.ids[1].public().sig_pk]
    );
    for m in 0..2 {
        assert_eq!(s.pools[&log.ids[m].public().sig_pk].available(), 0);
    }
    // The member outside every assigned group keeps its commitment.
    assert_eq!(s.pools[&log.ids[2].public().sig_pk].available(), 1);
    assert!(pre.groups_of(&log.ids[2].public().sig_pk).is_empty());
    // Nothing left for the next proposal: interactive.
    log.push(0, &proposal(2));
    let s = log.replay().unwrap();
    assert!(s.proposals[&[2; 16]].preprocessed.is_none());
}

#[test]
fn a_member_without_a_pool_does_not_take_one_tap_from_the_others() {
    let mut log = Log::new();
    log.push(0, &log.created(&[0, 1, 2]));
    let mut rng = StdRng::seed_from_u64(6);
    for m in 0..2 {
        log.push(m, &commitments(&mut rng, 4));
    }
    log.push(0, &proposal(1));
    let s = log.replay().unwrap();
    let pre = s.proposals[&[1; 16]].preprocessed.as_ref().unwrap();
    // Only {m0, m1} can sign one-tap.
    assert_eq!(pre.subsets.len(), 1);
    assert_eq!(pre.groups_of(&log.ids[0].public().sig_pk), vec![0]);
    assert!(pre.groups_of(&log.ids[2].public().sig_pk).is_empty());
    // Their shares complete the group; the third member isn't needed.
    log.push(0, &share_vote(1, &[0]));
    log.push(1, &share_vote(1, &[0]));
    let s = log.replay().unwrap();
    let p = &s.proposals[&[1; 16]];
    assert_eq!(p.ready_group, Some(0));
    assert_eq!(p.status, ProposalStatus::Approved);
    // A share vote for a group the member isn't in is still refused.
    log.push(2, &share_vote(1, &[0]));
    assert!(log.replay().unwrap().ignored.iter().any(|(_, e)| matches!(
        e,
        VaultError::BadShares(_) | VaultError::ProposalClosed(_) | VaultError::VoteFinal(_)
    )));
    // Nobody has a pool: interactive.
    let mut bare = Log::new();
    bare.push(0, &bare.created(&[0, 1, 2]));
    bare.push(0, &proposal(1));
    assert!(bare.replay().unwrap().proposals[&[1; 16]]
        .preprocessed
        .is_none());
}

/// The assignment is a function of the log, whoever replays it.
#[test]
fn partial_assignment_is_deterministic() {
    let mut log = pooled_log(3);
    log.push(0, &proposal(1));
    log.push(1, &proposal(2));
    let (a, b) = (log.replay().unwrap(), log.replay().unwrap());
    for id in [1u8, 2] {
        assert_eq!(
            a.proposals[&[id; 16]].preprocessed,
            b.proposals[&[id; 16]].preprocessed
        );
    }
}

#[test]
fn share_votes_complete_a_group_and_are_final() {
    let mut log = pooled_log(2);
    log.push(0, &proposal(1));
    // Groups: 0 = {m0,m1}, 1 = {m0,m2}, 2 = {m1,m2} (descriptor order).
    log.push(0, &share_vote(1, &[0, 1]));
    let s = log.replay().unwrap();
    assert_eq!(s.proposals[&[1; 16]].ready_group, None);
    log.push(2, &share_vote(1, &[1, 2]));
    let s = log.replay().unwrap();
    let p = &s.proposals[&[1; 16]];
    assert_eq!(p.status, ProposalStatus::Approved);
    assert_eq!(p.ready_group, Some(1));
    assert_eq!(p.completed_by, Some(log.ids[2].public().sig_pk));
    // Member 2 can no longer withdraw: the shares are out.
    log.push(2, &vote(1, false));
    let s = log.replay().unwrap();
    assert!(s
        .ignored
        .iter()
        .any(|(_, e)| matches!(e, VaultError::VoteFinal(_))));
    assert_eq!(s.proposals[&[1; 16]].status, ProposalStatus::Approved);
}

#[test]
fn invalid_share_votes_are_ignored() {
    let mut log = pooled_log(2);
    log.push(0, &proposal(1));
    log.push(0, &share_vote(1, &[2])); // member 0 is not in group 2 = {m1,m2}
    log.push(0, &share_vote(1, &[0, 0])); // the same group twice
    log.push(0, &share_vote(1, &[7])); // no such group
    let s = log.replay().unwrap();
    assert_eq!(
        s.ignored
            .iter()
            .filter(|(_, e)| matches!(e, VaultError::BadShares(_)))
            .count(),
        3
    );
    assert!(s.proposals[&[1; 16]].shares.is_empty());
}

#[test]
fn garbage_commitments_are_rejected() {
    let mut log = Log::new();
    log.push(0, &log.created(&[0, 1, 2]));
    log.push(
        1,
        &VaultEvent::Commitments {
            batch: vec![vec![1, 2, 3]],
        },
    );
    let s = log.replay().unwrap();
    assert!(matches!(s.ignored[0].1, VaultError::BadCommitments(_)));
    assert!(s.pools.is_empty() || s.pools.values().all(|p| p.commitments.is_empty()));
}

/// A proposal event carrying a real PCZT, built on `tip`.
fn proposal_with(id: u8, pczt: &pczt::Pczt, tip: u32) -> VaultEvent {
    VaultEvent::Proposal {
        id: [id; 16],
        payments: vec![],
        pczt: pczt.clone().serialize().unwrap(),
        pczt_hash: [id; 32],
        tip_height: tip,
        created_at: 1_700_000_000,
        signing_spends: 1,
        auto_send: false,
    }
}

#[test]
fn a_proposal_respending_notes_of_a_live_one_is_ignored() {
    use common::{build_pczt, outside_address, receive_ironwood_note, witness, Out};
    use orchard::keys::{FullViewingKey, Scope, SpendingKey};

    let fvk = FullViewingKey::from(&SpendingKey::from_bytes([3; 32]).unwrap());
    let spend = |note: orchard::Note| {
        let (anchor, path) = witness(&note);
        build_pczt(
            &fvk,
            note,
            path,
            anchor,
            vec![Out {
                ovk: None,
                recipient: outside_address(9),
                // The whole note minus the ZIP 317 fee (2 actions): no change output.
                value: 990_000,
                memo: zcash_protocol::memo::MemoBytes::empty(),
            }],
        )
    };
    let note_a = receive_ironwood_note(&fvk, fvk.address_at(0u32, Scope::External), 1_000_000);
    let note_b = receive_ironwood_note(&fvk, fvk.address_at(1u32, Scope::External), 1_000_000);
    let (a1, a2, b) = (spend(note_a), spend(note_a), spend(note_b));
    let tip = common::TARGET_HEIGHT - 1;
    let expiry = *b.global().expiry_height();
    assert!(expiry > tip);

    let mut log = Log::new();
    log.push(0, &log.created(&[0, 1, 2]));
    log.push(0, &proposal_with(1, &a1, tip));
    let state = log.replay().unwrap();
    assert_eq!(state.proposals[&[1; 16]].expiry_height, expiry);
    assert!(!state.proposals[&[1; 16]].nullifiers.is_empty());
    // Members check before appending, so a raced proposal is caught before it's written.
    assert_eq!(
        state.check(log.ids[1].public().sig_pk, &proposal_with(2, &a2, tip)),
        Err(VaultError::NotesInUse(2))
    );

    // Written anyway (two members raced): everyone ignores the second.
    log.push(1, &proposal_with(2, &a2, tip));
    log.push(1, &proposal_with(3, &b, tip));
    let state = log.replay().unwrap();
    assert!(!state.proposals.contains_key(&[2; 16]));
    assert!(state.proposals.contains_key(&[3; 16]));
    assert_eq!(state.ignored, vec![(2, VaultError::NotesInUse(2))]);

    // Past the earlier proposal's expiry its notes are free again.
    log.push(2, &proposal_with(4, &b, expiry));
    assert!(log.replay().unwrap().proposals.contains_key(&[4; 16]));

    // Wallet holds: live proposals in log order; a dropped broadcast releases its notes.
    log.push(0, &vote(3, true));
    log.push(2, &vote(3, true));
    log.push(
        0,
        &VaultEvent::Broadcast {
            proposal: [3; 16],
            txid: [7; 32],
        },
    );
    let state = log.replay().unwrap();
    assert_eq!(state.proposals[&[3; 16]].status, ProposalStatus::Broadcast);
    let owners = |dropped: &[u8]| {
        let dropped = dropped.iter().map(|d| [*d; 16]).collect();
        node::note_holds(&state, &dropped)
            .iter()
            .map(|h| h.owner[0])
            .collect::<Vec<_>>()
    };
    assert_eq!(owners(&[]), vec![1, 3, 4]);
    assert_eq!(owners(&[3]), vec![1, 4]);

    // A cancelled proposal holds nothing: its notes can be respent (to invalidate it).
    log.push(0, &VaultEvent::Cancelled { proposal: [1; 16] });
    log.push(1, &proposal_with(5, &a2, tip));
    let state = log.replay().unwrap();
    assert!(state.proposals.contains_key(&[5; 16]));
    assert!(node::note_holds(&state, &Default::default())
        .iter()
        .all(|h| h.owner[0] != 1));
}

/// Counts the nonces `forget_closed` deletes (every commitment counts as held here).
#[derive(Default)]
struct CountingPool(usize);

impl zafe_core::session::PoolStore for CountingPool {
    fn put(
        &mut self,
        _: &[u8],
        _: reddsa::frost::redpallas::round1::SigningNonces,
    ) -> Result<(), zafe_core::session::SessionError> {
        Ok(())
    }
    fn contains(&self, _: &[u8]) -> bool {
        true
    }
    fn take(&mut self, _: &[u8]) -> Option<reddsa::frost::redpallas::round1::SigningNonces> {
        None
    }
    fn forget(&mut self, _: &[u8]) {
        self.0 += 1;
    }
}

#[test]
fn nonces_of_expired_one_tap_proposals_are_forgotten() {
    use common::{build_pczt, outside_address, receive_ironwood_note, witness, Out};
    use orchard::keys::{FullViewingKey, Scope, SpendingKey};

    let fvk = FullViewingKey::from(&SpendingKey::from_bytes([3; 32]).unwrap());
    let note = receive_ironwood_note(&fvk, fvk.address_at(0u32, Scope::External), 1_000_000);
    let (anchor, path) = witness(&note);
    let pczt = build_pczt(
        &fvk,
        note,
        path,
        anchor,
        vec![Out {
            ovk: None,
            recipient: outside_address(9),
            value: 990_000,
            memo: zcash_protocol::memo::MemoBytes::empty(),
        }],
    );
    let expiry = *pczt.global().expiry_height();

    let mut log = pooled_log(2);
    log.push(0, &proposal_with(1, &pczt, common::TARGET_HEIGHT - 1));
    let state = log.replay().unwrap();
    assert!(state.proposals[&[1; 16]].preprocessed.is_some());
    let me = log.ids[0].public().sig_pk;
    let forgotten = |tip: Option<u32>| {
        let mut pool = CountingPool::default();
        node::forget_closed(&state, &me, tip, &mut pool)
    };
    assert_eq!(forgotten(None), 0, "tip unknown: keep");
    assert_eq!(forgotten(Some(expiry - 1)), 0, "still open");
    // Member 0 is in two of the three 2-of-3 groups, one spend each.
    assert_eq!(forgotten(Some(expiry)), 2);

    // Interactive nonces for the same proposal go too, once it expired.
    use zafe_core::session::{MemoryNonceStore, NonceStore};
    let mut store = MemoryNonceStore::default();
    let p = &state.proposals[&[1; 16]];
    store.put(p.id, p.pczt_hash, vec![]).unwrap();
    assert_eq!(
        node::forget_closed_nonces(&state, Some(expiry - 1), &mut store),
        0
    );
    assert!(store.contains(&p.id, &p.pczt_hash));
    assert_eq!(
        node::forget_closed_nonces(&state, Some(expiry), &mut store),
        1
    );
    assert!(!store.contains(&p.id, &p.pczt_hash));
}

#[test]
fn expiry_heights_are_rounded_up_to_the_grid() {
    use zafe_core::vault::{expiry_height, EXPIRY_ROUNDING_BLOCKS as R};
    // Proposals built a few blocks apart share one expiry: it doesn't date the proposal.
    let a = expiry_height(1_000_001, 8064);
    let b = expiry_height(1_000_050, 8064);
    assert_eq!(a, b);
    assert_eq!(a % R, 0);
    assert!((1_000_001 + 8064..1_000_001 + 8064 + R).contains(&a));
    // Already on the grid: unchanged.
    assert_eq!(expiry_height(R * 10, R), R * 11);
}

// --- Unapproved-spend alert ---------------------------------------------------------------

mod spend_watch_tests {
    use super::*;
    use zafe_core::{
        spend_watch::{unapproved_spends, UnapprovedSpend, GRACE_BLOCKS},
        wallet::VaultSpend,
    };

    fn state_with_proposal(nfs: &[[u8; 32]], txid: Option<[u8; 32]>) -> VaultState {
        let mut log = Log::new();
        log.push(0, &log.created(&[0, 1, 2]));
        log.push(0, &proposal(1));
        let mut s = log.replay().unwrap();
        let p = s.proposals.get_mut(&[1; 16]).unwrap();
        p.nullifiers = nfs.to_vec();
        p.txid = txid;
        s
    }

    fn spend(txid: u8, mined: Option<u32>, nfs: &[[u8; 32]]) -> VaultSpend {
        VaultSpend {
            txid: [txid; 32],
            mined_height: mined,
            nullifiers: nfs.to_vec(),
        }
    }

    const A: [u8; 32] = [0xA1; 32];
    const B: [u8; 32] = [0xB2; 32];

    #[test]
    fn a_spend_logged_as_sent_is_accounted_for_whenever_it_mined() {
        let s = state_with_proposal(&[A, B], Some([7; 32]));
        // Even long after: the log says this proposal's transaction was sent.
        assert!(unapproved_spends(&s, &[spend(7, Some(10), &[A, B])], 10_000).is_empty());
    }

    #[test]
    fn a_spend_no_proposal_explains_is_flagged_at_once() {
        let s = state_with_proposal(&[A], None);
        let c = [0xC3; 32];
        let flagged = unapproved_spends(&s, &[spend(9, None, &[c])], 100);
        assert_eq!(
            flagged,
            vec![UnapprovedSpend {
                txid: [9; 32],
                mined_height: None
            }]
        );
        // One unknown note among known ones is enough.
        assert_eq!(
            unapproved_spends(&s, &[spend(9, Some(99), &[A, c])], 100).len(),
            1
        );
    }

    #[test]
    fn a_proposals_spend_is_given_time_to_be_logged() {
        let s = state_with_proposal(&[A, B], None);
        // Sent but the broadcast entry isn't in the log yet: fine while recent...
        assert!(unapproved_spends(&s, &[spend(5, None, &[A])], 100).is_empty());
        assert!(
            unapproved_spends(&s, &[spend(5, Some(100), &[A, B])], 100 + GRACE_BLOCKS).is_empty()
        );
        // ...flagged once it has been mined for longer than the grace period with no
        // matching broadcast (a payment that was cancelled, or paid somewhere else).
        assert_eq!(
            unapproved_spends(&s, &[spend(5, Some(100), &[A, B])], 100 + GRACE_BLOCKS + 1).len(),
            1
        );
    }

    #[test]
    fn rows_of_one_transaction_are_one_spend_and_a_transaction_without_notes_is_flagged() {
        let s = state_with_proposal(&[A, B], None);
        // The wallet reports one row per spent note; together they match the proposal.
        let rows = [spend(5, None, &[A]), spend(5, None, &[B])];
        assert!(unapproved_spends(&s, &rows, 100).is_empty());
        let none = state_with_proposal(&[], None);
        assert_eq!(unapproved_spends(&none, &rows, 100).len(), 1);
    }
}
