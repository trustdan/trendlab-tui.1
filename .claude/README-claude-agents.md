# TrendLab Claude Code Bundle (v3.1)

This bundle is a ready-to-drop `.claude/` folder for TrendLab v3:
- **commands/**: `/project:*` slash commands (architecture, execution, orders, etc.)
- **agents/**: role-based experts (Rust, Ratatui, Aesthetics, Execution, Strategist, etc.)

## Install
```bash
unzip trendlab-claude-bundle-v3_1.zip
mv dot-claude-v3_1 .claude
mv .claude /path/to/trendlab/
```

## Structure
```
.claude/
├── CLAUDE.md
├── settings.json
├── commands/
│   ├── architecture.md
│   ├── signals.md
│   ├── orders.md
│   ├── execution.md
│   ├── position-mgmt.md
│   ├── robustness.md
│   ├── benchmark.md
│   ├── data.md
│   ├── testing.md
│   └── tui.md
└── agents/
    ├── rust-expert.md
    ├── ratatui-expert.md
    ├── aesthetic-designer.md
    ├── trend-following-strategist.md
    ├── execution-engineer.md
    ├── quant-validator.md
    ├── data-hygiene.md
    ├── test-engineer.md
    ├── performance-engineer.md
    ├── polars-expert.md
    ├── optimization-sweeper.md
    ├── reporting-artifacts.md
    └── product-ux.md
```

## Usage patterns

### Slash commands (scoped help)
```bash
claude /project:execution "how should gap fills work for StopMarket?"
claude /project:orders "design OCO brackets + cancel/replace"
claude /project:robustness "define promotion ladder thresholds"
claude /project:tui "pacman progress bar widget design"
```

### Role agents (consistent personality for a task)
Paste an agent file’s contents at the top of a Claude Code session, or say:
“Act as the agent in `.claude/agents/rust-expert.md` and help me refactor the hot loop.”

(Exact invocation depends on how you run Claude Code; these are designed to be copy/pasteable and composable.)
