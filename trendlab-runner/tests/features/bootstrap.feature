Feature: Bootstrap & Regime Resampling (Level 5)

  Background:
    Bootstrap is the final robustness gauntlet.
    Tests stability under temporal resampling, regime shifts, and universe changes.

  Scenario: Block bootstrap preserves temporal structure
    Given a strategy config with 2-year date range
    And Bootstrap with BlockBootstrap mode (block_size = 20 days)
    When 100 bootstrap trials run
    Then each trial uses resampled blocks
    And autocorrelation structure is preserved

  Scenario: Regime resampling tests subsample stability
    Given a strategy config with 3-year date range
    And Bootstrap with RegimeResampling mode
    When 200 bootstrap trials run
    Then each trial uses a random 6-12 month window
    And median Sharpe has low variance across regimes

  Scenario: Universe Monte Carlo tests instrument dependency
    Given a strategy config with 5 instruments
    And Bootstrap with UniverseMC mode (drop_rate = 0.3)
    When 150 bootstrap trials run
    Then some trials drop 1-2 instruments
    And at least 1 instrument is always present
    And stable strategy has low Sharpe variance

  Scenario: Mixed bootstrap combines all three approaches
    Given a strategy config
    And Bootstrap with Mixed mode
    When 300 bootstrap trials run
    Then approximately 33% use BlockBootstrap
    And approximately 33% use RegimeResampling
    And approximately 33% use UniverseMC

  Scenario: Bootstrap identifies overfitted strategy
    Given a strategy overfit to specific date range
    And Bootstrap with RegimeResampling mode
    When 200 bootstrap trials run
    Then IQR exceeds 0.4
    And candidate REJECTED (final level rejection)

  Scenario: Bootstrap identifies robust champion
    Given a strategy with consistent performance
    And Bootstrap with BlockBootstrap mode (500 trials)
    When bootstrap runs
    Then median Sharpe is above 2.0
    And IQR is below 0.2
    And 90% confidence interval is tight
    And candidate PROMOTED (final champion)

  Scenario: Universe resampling catches single-stock overfit
    Given a strategy with 10 instruments
    And one instrument dominates returns
    And Bootstrap with UniverseMC mode (drop_rate = 0.3)
    When 200 bootstrap trials run
    Then Sharpe drops significantly when dominant instrument is dropped
    And IQR is high
    And candidate REJECTED
