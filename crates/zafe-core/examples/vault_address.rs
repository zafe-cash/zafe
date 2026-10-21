//! Prints a regtest unified address and UFVK for a freshly generated (throwaway) vault key.
//! Used to probe regtest mining; not part of the product.

use orchard::keys::{FullViewingKey, Scope, SpendingKey};
use zafe_core::keys::{VaultKeys, VaultSecret};
use zcash_keys::address::UnifiedAddress;
use zcash_protocol::{consensus::BlockHeight, local_consensus::LocalNetwork};

fn main() {
    // A throwaway ak with even y, from an orchard key; stands in for a DKG group key.
    let ak: [u8; 32] = FullViewingKey::from(&SpendingKey::from_bytes([42; 32]).unwrap()).to_bytes()
        [..32]
        .try_into()
        .unwrap();
    let keys = VaultKeys::derive(&VaultSecret::from_bytes([1; 32]), &ak).unwrap();
    let address = keys.fvk().address_at(0u32, Scope::External);
    let ua = UnifiedAddress::from_receivers(Some(address), None, None, None, None).unwrap();
    let one = Some(BlockHeight::from_u32(1));
    let regtest = LocalNetwork {
        overwinter: one,
        sapling: one,
        blossom: one,
        heartwood: one,
        canopy: one,
        nu5: one,
        nu6: one,
        nu6_1: one,
        nu6_2: one,
        nu6_3: one,
        nu7: None,
    };
    println!("ua={}", ua.encode(&regtest));
    println!("ufvk={}", keys.ufvk().unwrap().encode(&regtest).unwrap());
}
