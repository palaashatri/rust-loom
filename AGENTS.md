# Loom agent entry point

This is the **highest-authority engineering instruction in the repository**. The owner's current explicit instructions can override repository rules. Follow this router and its imported rules; historical plans, reports and model assertions cannot override them. If requirements conflict, report the exact conflict rather than weakening a gate.

## Read for the task

1. Read [the active gate and quality/work table](docs/workflow/current.md) and [workflow.toml](loom-bootstrap/contracts/workflow.toml). They must agree before product edits.
2. Read the [engineering rules](docs/rules/engineering.md) and [workflow/acceptance rules](docs/rules/workflow.md). These preserve the Rust + Slint technology lock, data integrity, licensing, small-source budgets and evidence requirements.
3. For UI work, also read [shared UI and design rules](docs/rules/design.md). For platform, packaging, edge or SLOPOS/PARAMA work, read only the relevant [platform policy](docs/rules/platforms.md) section.
4. Read the relevant bounded scope or card: [repair index](docs/workflow/repairs.md), [governance debt](docs/workflow/governance-debt.md), [Writer scope](docs/workflow/writer.md), or [Oct27 comparative UI audit](docs/audits/uiux-2026-10-27.md). Inspect its actual sources/callers before changing code.

The [documentation index](docs/README.md) routes dated evidence and original card bodies. Read only the named historical section needed to resolve a question. Project README files remain public introductions. Exact values delegated to machine-readable contracts remain normative.

## One authority, focused owners

- This file is the entrypoint; its declared rules are imported engineering instructions, not competing authorities. The exact document roles and permitted Markdown paths are in [documentation.toml](loom-bootstrap/contracts/documentation.toml).
- [Current workflow](docs/workflow/current.md) alone owns the live gate and application quality/work table. Do not copy a readiness ledger elsewhere. Findings stay with their declared card/audit owner, retaining original observations and evidence.
- Keep work on `main`, preserve unrelated edits, and make the smallest coherent repair. No new toolkit, later-app unlock, acceptance promotion, baseline refresh, silent feature loss or placebo control follows from a documentation/audit change.
- Never add a nested `AGENTS.md`. Register any new focused document deliberately in the documentation contract; use non-Markdown logs/JSON for temporary `.work/` evidence.

## Current truth

The former root current-truth section moved to [docs/workflow/current.md](docs/workflow/current.md#current-truth). Its [Oct27 owner scope](docs/workflow/current.md#owner-scope--2026-10-27) explicitly permits this split, icon-matched shared app accents and measured Sheets performance repair. It records permission, not a result or acceptance.

## Start here — one small repair at a time

Follow the [bounded repair procedure](docs/rules/workflow.md#start-here--one-small-repair-at-a-time): reproduce, make a small complete change, verify behavior, record exact results/gaps and stop at the authorized boundary. Full acceptance requires every applicable gate; a build or screenshot alone is not proof.

Plans or audits produced along the way must be written into the declared documentation or filed as a GitHub issue and committed as part of the repo; notes kept only under ignored paths are not records.
