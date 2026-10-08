//! Full, read-only account definition and running-balance parity.
//! The caller provides both genesis and history reference: their external
//! authenticity, event completeness and tenant authorization are NOT proven.

use cofi_ledger::Ledger;

use crate::CodecError;

/// Require exact original account inventory in all three ledgers, including
/// accounts untouched by the replayed financial journals. This bounded
/// replay does not support account registrations between staged checkpoints.
pub(crate) fn verify_all_accounts(
    genesis: &Ledger,
    reconstructed: &Ledger,
    reference: &Ledger,
) -> Result<(), CodecError> {
    if !genesis.accounts().eq(reconstructed.accounts())
        || !genesis.accounts().eq(reference.accounts())
    {
        return Err(CodecError::Replay(
            "historical account inventory added, removed or changed".into(),
        ));
    }
    for account in genesis.accounts() {
        let id = account.id();
        if reconstructed.balance(id).is_none() || reconstructed.balance(id) != reference.balance(id)
        {
            return Err(CodecError::Replay(
                "historical account balance differs, including untouched account".into(),
            ));
        }
    }
    Ok(())
}
