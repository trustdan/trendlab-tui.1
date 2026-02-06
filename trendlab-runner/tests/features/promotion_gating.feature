Feature: Promotion gating (saves compute budget)

  Background:
    Cheap tests filter many candidates before expensive validation runs.

  Scenario: Failing Cheap Pass never consumes Execution MC budget
    Given 1000 strategy candidates
    When Level 1 (Cheap Pass) runs with threshold 1.0
    Then 100 candidates promote to Level 2

  Scenario: Promotion ladder filters progressively
    Given 1000 strategy candidates
    When Level 1 (Cheap Pass) runs with threshold 1.0
    And Level 2 (Walk-Forward) runs
    And Level 3 (Execution MC) runs
    Then 100 candidates promote to Level 2
    And 20 candidates promote to Level 3
    And 5 candidates reach final level
    And compute budget saved: 90%
