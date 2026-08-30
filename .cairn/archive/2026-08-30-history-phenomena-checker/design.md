# Design: History Phenomena Checker

## Context

Round logs and workload records capture operations with ordering and identity. The SMR workload detects divergence. This component classifies it. The named phenomena come from the shared reliability glossary vocabulary used by Antithesis and Jepsen.

## Decisions

### 1. Phenomena are typed and enumerated

The core defines operations, dependencies, and the supported phenomenon classes: aborted read, intermediate read, garbage read, stale read, lost write, and write cycle. Each class has an explicit check procedure.

### 2. Cycle detection is the first pass

Dependency-graph cycle detection runs in linear time and localizes a violation to a small operation set. This is the Elle technique. It is the only pass in this change.

### 3. A constraint pass is deferred

An optional constraint pass for histories whose safety depends on the absence of any legal interpretation is a later change. This change states that limitation instead of implementing a partial solver.

### 4. The core is pure

The core receives typed histories and returns typed violations. The shell reads round and log artifacts, assembles histories, and validates identities.

### 5. Evidence binds the diagnosis

Typed violations bind to the history identity and the operation records that produced them. Receipt validation fails closed on identity drift.

## Risks

A history with gaps produces either a missed phenomenon or a bounded insufficient-data result. The core must choose the bounded result. Cycle detection is exact only for the operations it can observe, because a missing observer hides part of the history. The checker must report observation bounds with every result.
