# Loom documentation map

Start with the root [agent entrypoint](../AGENTS.md). The owner requested this split on 2026-10-27 so agents can read a bounded task context. Rules and records were moved, not replaced with weaker summaries. Exact roles and permitted Markdown paths are declared in [documentation.toml](../loom-bootstrap/contracts/documentation.toml).

| Need | Authoritative owner |
|---|---|
| Current phase, permitted apps, quality/work statuses and current owner scope | [Current workflow](workflow/current.md) plus [workflow.toml](../loom-bootstrap/contracts/workflow.toml) |
| Technology, architecture, small-source budgets, licensing and truthful implementation | [Engineering rules](rules/engineering.md) |
| One-card repairs, test policy and binary acceptance gates | [Workflow rules](rules/workflow.md) |
| Shared foundation, accessibility, interaction and visual quality | [Design rules](rules/design.md) |
| Platform lifecycle, future SLOPOS/PARAMA and edge constraints | [Platform policy](rules/platforms.md) — read the relevant section only |
| Original CODE/UI/REC/PERF cards and current repair states | [Repair index](workflow/repairs.md), with original bodies in the named archive sections |
| GOV findings, including the bounded app-accent decision | [Governance debt](workflow/governance-debt.md) |
| Approved Writer M1–M7 scope | [Writer scope](workflow/writer.md) |
| Previously recorded capabilities worth preserving | [Capability inventory](workflow/capabilities.md); earlier results are not fresh acceptance |
| Oct27 Pages/Numbers/Keynote comparison, menu findings and icon verification | [Comparative audit](audits/uiux-2026-10-27.md) |

There is one live readiness ledger, in `workflow/current.md`. A card/audit owns its findings; do not copy status tables between files. Dates, test failures, retractions and native-versus-renderer limits remain part of the evidence. Historical desktop permissions and model-routing choices apply only to their original runs. No migration or old green report grants application acceptance.

## Targeted historical evidence

Open only the named section needed to establish an original finding, owner grant or recorded result. These files are not startup reading or a second work queue.

| Record | Contents |
|---|---|
| [Original archive](archive/AGENTS-ARCHIVE.md) | Full repair cards, archived owner overrides, design-bible and original source-path documents |
| [Initial audit evidence](archive/audit-evidence-2026-09-14.md) | Recorded code/UI audit and evidence boundaries |
| [Product evidence, Sep27–Oct09](archive/product-evidence-2026-09-27-to-10-09.md) | Earlier source, native, interop and performance checks; superseded statuses remain dated |
| [Windows/UI evidence, Sep30](archive/windows-ui-2026-09-30.md) | Scroll, layout, design-system and early office workflow records |
| [Continuation, Oct04–25](archive/continuation-2026-10-04-to-25.md) | Performance/feature history, owner v1 scope, acceptance claims and their explicit retractions |
| [Verification, Oct18–20](archive/verification-2026-10-18-to-20.md) | Scoped Writer/Sheets/Present repair results, failures and review gaps |
| [Writer checkpoints, Oct25–27](archive/writer-checkpoints-2026-10.md) | M1–M3 work, partial checks, recovery/history follow-ups and time-limited display permissions |

Migration evidence is in `.work/docs-split-20261027/`: the pre-split source snapshot, exact line-range hashes, final file hashes, caller/link checks and focused governance outputs. Ignored evidence is not a second authority.
