# REC-02 recovery format and limits — owner review

**Status: proposal only.** No recovery format or numeric limit in this document is approved or implemented. REC-02 remains open until the owner reviews the storage policy and recovery guarantees below. Existing recovery data must remain readable while a new design is introduced.

The measured million-cell test spent 12.72 seconds creating a full recovery package and 43.05 seconds appending it to the journal. It did not measure how many bytes a long editing session retains. The values below are proposed starting points, not measured product guarantees.

| Decision | Proposed default | What it means |
|---|---:|---|
| Recovery format | One complete package, followed by ordered, versioned cell-edit batches | Cell edits are small journal records. Structural changes initially write a new complete package. |
| Total retained recovery | 640 MiB | Sum of checkpoints, journal, metadata, and any legacy data kept during migration. |
| Temporary disk peak | 1 GiB | Includes old and new checkpoints, replacement journal, and temporary files while publishing safely. |
| Largest recoverable complete package | 256 MiB encoded | A larger workbook remains editable and can be saved by the user, but automatic recovery must say it is unavailable until the policy is changed. |
| Journal size | 64 MiB | Reaching the cap starts checkpointing; it must never delete the only valid recovery state. |
| Journal records | 10,000 total, 1 MiB per record | One oversized transaction uses the full-checkpoint path. A transaction must not be silently split. |
| Checkpoint trigger | 16 MiB, 2,000 records, or 5 minutes of edits | Use whichever limit is reached first, and checkpoint after a successful explicit save. |
| Checkpoint retention | Two complete generations; one extra only during publication | Publish and verify the new generation before removing the one it replaces. |
| Edit durability target | 250 ms p95 on the measured host | Sync the accepted edit batch before waiting for slow formula evaluation. Measure and report the worst case too. |

Each batch should contain a format version, workbook/session identity, baseline identity, predecessor durable sequence, and exact cell assignments or deletions. Keep UI revision numbers separate from recovery sequence numbers. Validate a whole batch before replaying it. Store formulas as formulas, and preserve complete packages for images and workbook structure.

Use one writer for each recovery session, with distinct session and workbook identifiers. Do not allow two app instances to write into the same ordered journal. A checkpoint for sequence N must never remove data for an accepted sequence greater than N.

Publish checkpoints in this order:

1. Choose the exact durable sequence covered by the new checkpoint.
2. Write its package and metadata, sync both, and verify them.
3. Atomically publish and sync the checkpoint pointer.
4. Replace the journal with records newer than the covered sequence.
5. Remove obsolete generations and temporary files only after the new state is safe.

On disk-full, cap, append, sync, or compaction failure, preserve the previous valid recovery data. Keep the in-memory workbook. Show **“Recent changes are not protected by recovery”** with **Retry** and **Save As** actions. Do not advance the durable sequence, discard a failed edit, or claim that the edit is recoverable. If bounded pending changes cannot be retained, stop accepting edits until a complete checkpoint succeeds.

Read legacy full-package journals without changing their format. To migrate, first publish and verify a complete baseline in the new versioned session directory; only then make the new format active. Count legacy bytes against the storage limit. If migration cannot fit, leave the old data untouched and report the limitation. Older binaries must not write into the new journal.

Required failure and correctness tests include exact restart reconstruction after cell edits, blank-cell deletions, formulas, tabs, undo/redo, replacements, and images; interruption after every write, sync, pointer change, compaction, and cleanup; torn tails and corrupt middle records; unsupported versions and wrong lineage; duplicate batches and failed writes; disk-full and cap enforcement during migration; two app instances; repeated failed compactions; and the same million-cell workload measuring durability latency, replay time, retained bytes, peak temporary bytes, and RSS.

## Decisions needed from the owner

1. Approve the ordered cell-batch format, with structural changes initially stored as full packages.
2. Approve or replace the proposed 640 MiB retained limit, 1 GiB temporary peak, 256 MiB package limit, 64 MiB / 10,000-record journal limits, and 1 MiB record limit.
3. Approve or replace the 16 MiB / 2,000-record / 5-minute checkpoint triggers, two-generation retention, and 250 ms p95 durability target.
4. Confirm the recovery-error wording and that editing may pause if the bounded recovery queue cannot safely keep up.

After those decisions, implementation should be split into small changes: versioned record encoding and replay; crash-safe checkpoint publication; legacy migration; bounded storage and truthful error UI; then failure injection and the unchanged performance workload. Do not mark REC-02 fixed until every step has measured, passing evidence.
