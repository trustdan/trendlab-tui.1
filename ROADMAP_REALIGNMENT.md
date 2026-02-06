# Roadmap Realignment Map (M9-M12)

This document maps the current discrepancies to the roadmap's original intent and
the acceptance criteria needed to bring TrendLab v3 back on track.

## Source of truth

- Roadmap: `trendlab-v3-development-roadmap-bdd-v2.md` (M9-M12 and Checkpoint C)
- Discrepancies: `DISCREPANCY_REPORT.md`

## Gap-to-Deliverable Map

| Gap (Discrepancy Report) | Roadmap Requirement | Acceptance Criteria |
| --- | --- | --- |
| Path MC scaffolded | M9 robustness ladder (escape hatch says "add Path MC later") | Path MC runs distinct intrabar policies and produces a distribution of results |
| Bootstrap scaffolded | M9 robustness ladder / M11 bootstrap spec | Bootstrap resamples time blocks and/or regimes and produces distribution + CI |
| Ghost curve missing | M10 TUI + Checkpoint C | Ghost curve visible in TUI; divergence metric computed; >15% flagged |
| Drill-down placeholders | M10 drill-down flow | Can trace signal → intent → order → fill (summary, trade tape, chart, diagnostics) |
| Rejected intents absent | M10 deliverables + Checkpoint C | Rejected intents logged and displayable for 4 rejection types |
| TUI data loading missing | M10 core panels | TUI loads and renders results (from cache or result directory) |
| Reporting/artifacts missing | M11 reporting & artifacts | Manifests + equity + trades + diagnostics exported; optional markdown report |

## Checkpoint C (after M10)

All three must be satisfied in TUI:

1. **Death Crossing Analysis**: Ghost Curve vs Real Curve divergence >15% flagged.
2. **Rejected Intent Coverage**: 4 rejection types shown (VolatilityGuard, LiquidityGuard, MarginGuard, RiskGuard).
3. **Drill-Down Completeness**: trade traceable from signal → intent → order → fill.

## M12 Hard-Fail Tests (revalidation after fixes)

These must remain green after remediation:

1. **Concurrency Torture**: 16-thread sweep matches 1-thread sweep bit-for-bit.
2. **Death Crossing**: Ghost curve divergence detection exercised and logged.
3. **Cache Mutation**: delete cache, rerun, equity curve reproduces exactly.

## Notes

- Yahoo Finance integration is not in M9-M12 and was explicitly deferred in M2.
  It should be tracked as a post-M12 milestone if still desired.
