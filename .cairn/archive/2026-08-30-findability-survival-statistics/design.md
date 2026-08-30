# Design: Findability Survival Statistics

## Context

Round traces, verdict records, and bug reports exist. The exploration tree splits at the root moment where fault injection starts, and each branch is a subtree. The causality engine attributes failures to candidate causes. Findability statistics answer a different question: how confident can we be that a rare bug is gone.

## Decisions

### 1. The model is exponential per subtree

The core counts only the first bug instance per subtree. It fits the bug rate as M over T, where M is the count of first-bug instances and T is total survival time. The mean time-to-bug is T over M. This follows the survival-analysis approach documented by Antithesis for findability curves.

### 2. Uncertainty uses a gamma prior

A small M makes the rate uncertain. The core places a gamma prior on the rate and reports the Lomax posterior survival curve. The tail is deliberately conservative: it understates confidence rather than overstating it.

### 3. Independence is explicit

Bugs in different branches of one subtree are not independent. The core counts one instance per subtree. If a bug is baked into every subtree, the core detects the independence violation and flags it instead of reporting a false confidence.

### 4. The core is pure

The core receives typed observations and returns model outputs. The shell reads round and verdict artifacts, assembles observations, and validates binding identities.

### 5. Evidence binds inputs and outputs

The receipt carries the observation set identity, the model parameters, and the outputs, bound by BLAKE3. Receipt verification fails closed on identity drift.

## Risks

A model is only as honest as its stated assumptions. The exponential-per-subtree model assumes discovery at a constant rate, and the report must state that assumption. A changed codebase between runs shifts the rate, so the shell must weight or split data across run generations.
