Feature: Robustness ladder integration

  Background:
    End-to-end validation of the complete robustness ladder.

  Scenario: Multiple candidates run through full ladder
    Given 3 test strategy configs
    When robustness ladder runs with 3 levels
    Then each candidate produces level results
    And results include stability scores and distributions
