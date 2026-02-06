Feature: Stability-aware promotion

  Background:
    Stability scoring penalizes variance to reward consistent strategies.
    Formula: score = median - penalty_factor * IQR

  Scenario: High variance candidate vs stable candidate
    Given Candidate A: median sharpe = 2.5, IQR = 1.0
    And Candidate B: median sharpe = 2.0, IQR = 0.3
    And promotion filter: penalty_factor = 0.5, min_stability_score = 1.5
    When stability scores computed
    Then Candidate A has stability score 2.0
    And Candidate B has stability score 1.85

  Scenario: Low IQR candidate promoted over high median unstable one
    Given Candidate A: median sharpe = 2.5, IQR = 1.0
    And Candidate B: median sharpe = 2.0, IQR = 0.2
    And promotion filter: penalty_factor = 0.5, min_stability_score = 1.5
    And max_iqr threshold = 0.5
    When stability scores computed
    And promotion filter applied
    Then Candidate A REJECTED
    And Candidate B PROMOTED
