## ADDED Requirements

### Requirement: Deduplicate bugs by assertion and fault signature

The explorer SHALL deduplicate bugs so that the same assertion failing
under the same category of faults is recorded only once in the corpus.

#### Scenario: Duplicate DiskFull bugs collapsed

- **WHEN** bug A has assertion_id=X and fault types={DiskFull}
- **WHEN** bug B has assertion_id=X and fault types={DiskFull}
- **THEN** only one bug is added to the corpus
- **THEN** the bug count reflects unique (assertion, fault_signature) pairs

#### Scenario: Different fault types kept separate

- **WHEN** bug A has assertion_id=X and fault types={DiskFull}
- **WHEN** bug B has assertion_id=X and fault types={NetworkPartition}
- **THEN** both bugs are added to the corpus as distinct entries

#### Scenario: No-fault bugs kept separate

- **WHEN** bug A has assertion_id=X and fault types={}
- **WHEN** bug B has assertion_id=X and fault types={DiskFull}
- **THEN** both bugs are added as distinct entries (no-fault vs faulted
  failures are independently interesting)

### Requirement: Dedup hash reported in bug output

The explorer SHALL include the dedup key in bug report JSON output so
that users can see which bugs are considered equivalent.

#### Scenario: Dedup key in JSON

- **WHEN** a bug report is saved as JSON
- **THEN** the JSON includes a `dedup_key` field containing the
  (assertion_id, sorted_fault_types) hash
