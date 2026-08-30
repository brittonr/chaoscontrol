# Design: Fallback Assertion Transport

## Context

The property oracle validates and aggregates catalog-bound assertion events. The SDK emits them through a shared page. Non-Rust processes have no path into the oracle. The WAL exercise used a fallback JSONL sink for this exact reason.

## Decisions

### 1. The record format is a declared line schema

A fallback record is one self-contained line with a type, a stable logical key, a condition result where applicable, a message, and a process identity. The schema is versioned and owned by the protocol crate.

### 2. Ingestion is deterministic

A process writes records to a deterministic sink, for example a fixed path on shared storage or a dedicated ring exposed by the supervisor. The host ingests the sink in record order. Record order is part of replay identity.

### 3. Fallback records bind to catalog identities

The stable logical key and message derive to the assertion catalog fingerprint under the existing identity rules. An unknown or conflicting key produces a typed catalog event, never silent acceptance.

### 4. Sinks are bounded and fail closed

A bounded sink emits a typed overflow event instead of silently dropping records. Malformed records are rejected with a typed diagnostic that names the record and the owning process.

### 5. Coverage remains instrumentation-bound

Fallback is an assertion and lifecycle path only. Code coverage still requires SanCov hooks, which the SDK provides for linked Rust code. A third-party build must link or supply those hooks for coverage evidence.

## Risks

A fallback path two processes share can interleave records. The sink order must be the authority, so the process identity field is mandatory and ordering is validated at ingestion. Schema drift between SDK and fallback-sourced records must fail the catalog, not weaken it.
