use super::{
    checked_next_sequence, reconcile_checkpoint_generations_with_limits, repair_journal,
    JournalRecord, ProductionError, RecoveryDirectoryLock, RecoveryInspectionCursor,
    RecoveryInspectionLimits, RecoveryJournal,
};

impl RecoveryJournal {
    /// Open or repair a journal using finite checkpoint and journal read ceilings.
    ///
    /// This path loads each checkpoint through an opened, size-checked file handle,
    /// streams the bounded journal, and only then repairs a torn tail or prunes
    /// checkpoint generations. The caller must hold the directory lock throughout.
    pub fn open_with_recovery_lock_limited(
        recovery_lock: &RecoveryDirectoryLock,
        limits: RecoveryInspectionLimits,
    ) -> Result<Self, ProductionError> {
        let directory = recovery_lock.directory.clone();
        let mut cursor = RecoveryInspectionCursor::open_with_limits(&directory, limits)?;
        let mut records = Vec::new();
        while let Some(record) = cursor.next_record()? {
            records.push(record);
        }
        Self::open_from_inspection(recovery_lock, cursor, &records)
    }

    /// Finalize the exact bounded snapshot already inspected and replayed by
    /// the caller. The caller must pass every verified record, unchanged and
    /// in the order yielded by the cursor. Snapshot verification completes
    /// before a torn tail is repaired or checkpoint generations are pruned.
    pub fn open_from_inspection(
        recovery_lock: &RecoveryDirectoryLock,
        cursor: RecoveryInspectionCursor,
        records: &[JournalRecord],
    ) -> Result<Self, ProductionError> {
        let directory = recovery_lock.directory.clone();
        let limits = cursor.limits();
        cursor.validate_finalization_input(&directory, records)?;
        let stats = *cursor
            .stats()
            .expect("inspection cursor reaches EOF before journal finalization");
        let journal_sequence = cursor.last_sequence();
        let checkpoint_state = cursor.into_checkpoint_state_after_eof()?;

        let checkpoint_sequence = checkpoint_state
            .metadata
            .as_ref()
            .map(|metadata| metadata.last_sequence);
        let highest_sequence = journal_sequence
            .into_iter()
            .chain(checkpoint_sequence)
            .max();
        let next_sequence = match highest_sequence {
            Some(sequence) => checked_next_sequence(sequence)?,
            None => 1,
        };

        if stats.incomplete_tail {
            repair_journal(&directory, records)?;
        }
        reconcile_checkpoint_generations_with_limits(&directory, &checkpoint_state, limits)?;

        Ok(Self {
            directory,
            next_sequence,
        })
    }
}
