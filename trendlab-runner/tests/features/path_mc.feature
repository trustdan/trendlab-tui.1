Feature: Path Monte Carlo (Level 4)

  Background:
    Intrabar ambiguity arises when a bar's range includes multiple trigger levels.
    PathMC tests strategy robustness to different execution order assumptions.

  Scenario: Strategy robust to path assumptions
    Given a strategy config with bracket orders
    And PathMC with 100 trials using Random sampling mode
    When PathMC runs
    Then median Sharpe is above 1.5
    And IQR is below 0.3
    And candidate PROMOTED to Level 5

  Scenario: Strategy sensitive to path assumptions
    Given a strategy config with tight stops
    And PathMC with 100 trials using Random sampling mode
    When PathMC runs
    Then IQR exceeds 0.5
    And candidate REJECTED

  Scenario: WorstCase vs BestCase baseline comparison
    Given a strategy config with bracket orders
    When PathMC runs with WorstCase mode (50 trials)
    And PathMC runs with BestCase mode (50 trials)
    Then WorstCase median Sharpe <= BestCase median Sharpe
    And BestCase - WorstCase delta quantifies optimism bias

  Scenario: Mixed sampling includes all three policies
    Given PathMC with Mixed sampling mode
    When 300 trials run
    Then approximately 33% use WorstCase policy
    And approximately 33% use BestCase policy
    And approximately 33% use Deterministic policy

  Scenario: Path sensitivity identifies ambiguous bar dependency
    Given a strategy that triggers many ambiguous bars
    And PathMC with 200 trials using Random sampling mode
    When PathMC runs
    Then IQR is high (> 0.5)
    And rejection reason is "High path sensitivity"
