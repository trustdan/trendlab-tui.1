# TrendLab v3 Architecture Invariants

## 1. Separation of Concerns

- **Signals** are portfolio-agnostic
- **Position management** is post-execution only
- **Execution** is configurable and realistic

## 2. Bar-by-Bar Event Loop

Per bar:
1. Start-of-bar: activate day orders, fill MOO
2. Intrabar: simulate triggers/fills via path policy
3. End-of-bar: fill MOC
4. Post-bar: mark positions, PM emits intents for NEXT bar

## 3. Deterministic Reproducibility

Every run keyed by: config hash + dataset hash + seed → exact results

## 4. Execution Realism

- Gap rule: stops gapped through fill at open (worse)
- Ambiguity rule: WorstCase default (adversarial ordering)
- No "perfect touch" assumptions
