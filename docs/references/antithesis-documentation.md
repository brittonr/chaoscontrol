# Antithesis Documentation

> Antithesis is an autonomous software testing platform that finds deep bugs using deterministic simulation and continuous fuzzing. This index links to each documentation page in raw Markdown form.

Captured for ChaosControl design and implementation work on 2026-07-28.

This material is a design reference, not a ChaosControl requirement or parity claim. ChaosControl keeps its documented product boundaries. These boundaries include Rust-only guests and no Docker, OCI, Compose, or Kubernetes intake.

WalTier DST is a second bounded comparison source at the object-store seam. See [the WalTier DST record](waltier-dst.md) for its separate mechanism and claim boundaries.

Neither source creates a ChaosControl requirement or parity claim. Existing repository policy and evidence gates remain authoritative.

## Introduction

- [Welcome to Antithesis](https://antithesis.com/docs/introduction/welcome.md): Explore the guides and examples for the Antithesis autonomous testing platform.
- [How Antithesis works](https://antithesis.com/docs/introduction/how_antithesis_works.md): Autonomous testing that generates test cases, explores system states, and reproduces bugs deterministically.
- [Using Antithesis with AI](https://antithesis.com/docs/introduction/using_antithesis_with_ai.md): Use AI to get answers about Antithesis.

## Get started

- [Setup guide](https://antithesis.com/docs/getting_started/setup_guide.md): Package and configure software with Docker Compose or Kubernetes.
- [Docker Compose setup guide](https://antithesis.com/docs/getting_started/setup_guide/docker_compose.md): Configure software for testing in the Antithesis Docker environment.
- [Kubernetes setup guide](https://antithesis.com/docs/getting_started/setup_guide/setup_k8s.md): Configure software for testing in the Antithesis Kubernetes environment.
- [Test an example system](https://antithesis.com/docs/getting_started/tutorials.md)
- [With Docker Compose](https://antithesis.com/docs/getting_started/tutorials/docker_compose.md)
- [Build and run an etcd cluster](https://antithesis.com/docs/getting_started/tutorials/docker_compose/cluster-setup.md)
- [Add a test template](https://antithesis.com/docs/getting_started/tutorials/docker_compose/cluster-test.md)
- [With Kubernetes](https://antithesis.com/docs/getting_started/tutorials/kubernetes.md)
- [Build and run an etcd cluster](https://antithesis.com/docs/getting_started/tutorials/kubernetes/k8s-cluster-setup.md)
- [Add a test template](https://antithesis.com/docs/getting_started/tutorials/kubernetes/k8s-cluster-test.md)

## Concepts

- [Properties and Assertions](https://antithesis.com/docs/concepts/properties_assertions/overview.md): An overview of properties and assertions.
- [Properties in Antithesis](https://antithesis.com/docs/concepts/properties_assertions/properties.md): Always and sometimes properties, default and custom properties, and property analysis.
- [Assertions in Antithesis](https://antithesis.com/docs/concepts/properties_assertions/assertions.md): Assertion messages and language-specific implementations.
- [Sometimes Assertions](https://antithesis.com/docs/concepts/properties_assertions/sometimes_assertions.md): Use sometimes assertions to measure code reachability and test coverage.
- [Properties to test for](https://antithesis.com/docs/concepts/properties_assertions/reliability_properties.md): Functional and concurrency properties for distributed systems, including ACID, CAP, and fault models.
- [Fault injection overview](https://antithesis.com/docs/concepts/fault_injection.md): Network faults, node failures, and thread pauses.
- [Types of faults](https://antithesis.com/docs/concepts/fault_injection/fault_types.md)
- [Pausing faults](https://antithesis.com/docs/concepts/fault_injection/pause_faults.md)
- [Fault events in logs and reports](https://antithesis.com/docs/concepts/fault_injection/fault_logs.md)

## Product

- [Test templates](https://antithesis.com/docs/product/test_templates.md)
- [Creating test templates](https://antithesis.com/docs/product/test_templates/first_test.md)
- [Test commands](https://antithesis.com/docs/product/test_templates/test_composer_reference.md)
- [How to check a test template locally](https://antithesis.com/docs/product/test_templates/testing_locally.md)
- [How to port tests to Antithesis](https://antithesis.com/docs/product/test_templates/composer_example.md)
- [Test launchers](https://antithesis.com/docs/product/test_launchers.md): Configure and launch Antithesis tests from the web application.
- [The triage report](https://antithesis.com/docs/product/reports.md)
- [Findings](https://antithesis.com/docs/product/reports/findings.md)
- [Environment](https://antithesis.com/docs/product/reports/environment.md)
- [Utilization](https://antithesis.com/docs/product/reports/utilization.md)
- [Properties](https://antithesis.com/docs/product/reports/properties.md)
- [Logs Explorer and multiverse map](https://antithesis.com/docs/product/logs_explorer.md): Search and show logs across multiple execution paths.
- [Debugging](https://antithesis.com/docs/product/debugging.md): Causality analysis and multiverse debugging.
- [Causality analysis](https://antithesis.com/docs/product/debugging/causality_analysis.md)
- [Simple Multiverse debugging](https://antithesis.com/docs/product/debugging/simple_mvd.md): Time-travel and destructive analysis.
- [Advanced mode](https://antithesis.com/docs/product/debugging/advanced_multiverse_debugging/overview.md)
- [The Antithesis multiverse](https://antithesis.com/docs/product/debugging/advanced_multiverse_debugging/moment_branch.md)
- [Querying with event sets](https://antithesis.com/docs/product/debugging/advanced_multiverse_debugging/event_sets.md)
- [Environment utilities](https://antithesis.com/docs/product/debugging/advanced_multiverse_debugging/bash_env.md)
- [Using the Antithesis Notebook](https://antithesis.com/docs/product/debugging/advanced_multiverse_debugging/antithesis_notebook.md)
- [Cookbook notebook](https://antithesis.com/docs/product/debugging/advanced_multiverse_debugging/cookbook.md)
- [CI integration](https://antithesis.com/docs/product/tooling_integrations/ci.md)
- [Discord and Slack integrations](https://antithesis.com/docs/product/tooling_integrations/discord_slack.md)
- [Issue tracker integration - BETA](https://antithesis.com/docs/product/tooling_integrations/issue_tracker_integration.md)

## Configuration

- [Access and authentication](https://antithesis.com/docs/configuration/auth.md): SSO, machine credentials, and report access.
- [The Antithesis environment](https://antithesis.com/docs/configuration/the_antithesis_environment.md): The machine and container environments that run the software.

## Best practices

- [Docker best practices](https://antithesis.com/docs/best_practices/docker_best_practices.md): Docker Compose practices and an example file.
- [Kubernetes best practices](https://antithesis.com/docs/best_practices/k8s_best_practices.md)
- [Optimizing for testing](https://antithesis.com/docs/best_practices/optimizing.md): Configure systems to exercise rare code paths more often.

## Reference

- [REST API](https://antithesis.com/docs/reference/rest_api.md): Programmatic access to the Antithesis platform.
- [Event sets](https://antithesis.com/docs/reference/event-set-reference.md)
- [Logs Explorer search](https://antithesis.com/docs/reference/logs_explorer_search.md): Search fields, operators, and preset values.
- [Coverage instrumentation](https://antithesis.com/docs/reference/instrumentation/coverage_instrumentation.md): Antithesis coverage instrumentation.
- [SDKs](https://antithesis.com/docs/reference/sdk.md): SDK integration reference.
- [Define test properties](https://antithesis.com/docs/reference/sdk/define_test_properties.md): Express properties as assertions that do not stop the program.
- [Generate randomness](https://antithesis.com/docs/reference/sdk/generate_randomness.md): Request structured randomness from Antithesis.
- [Manage test lifecycle](https://antithesis.com/docs/reference/sdk/manage_test_lifecycle.md)
- [Assertion catalog](https://antithesis.com/docs/reference/sdk/assertion_cataloging.md): Catalog all assertions in the system under test.
- [Go SDK](https://antithesis.com/docs/reference/sdk/go.md)
- [Go instrumentor tool](https://antithesis.com/docs/reference/sdk/go/instrumentor.md)
- [Go SDK tutorial, times10 function](https://antithesis.com/docs/reference/sdk/go/example.md)
- [Assert (Go reference)](https://antithesis.com/docs/reference/sdk/go/assert.md)
- [Lifecycle (Go reference)](https://antithesis.com/docs/reference/sdk/go/lifecycle.md)
- [Random (Go reference)](https://antithesis.com/docs/reference/sdk/go/random.md)
- [Java SDK](https://antithesis.com/docs/reference/sdk/java.md)
- [Using the Java SDK](https://antithesis.com/docs/reference/sdk/java/how_to_use_sdk.md)
- [Building software that uses the Java SDK](https://antithesis.com/docs/reference/sdk/java/how_to_build_with_sdk.md)
- [Java SDK tutorial, times10 method](https://antithesis.com/docs/reference/sdk/java/example.md)
- [Assert (Java reference)](https://antithesis.com/docs/reference/sdk/java/assert.md)
- [Lifecycle (Java reference)](https://antithesis.com/docs/reference/sdk/java/lifecycle.md)
- [Random (Java reference)](https://antithesis.com/docs/reference/sdk/java/random.md)
- [C SDK](https://antithesis.com/docs/reference/sdk/c_sdk.md)
- [C++ SDK](https://antithesis.com/docs/reference/sdk/cpp.md)
- [C/C++ instrumentation](https://antithesis.com/docs/reference/sdk/cpp/instrumentation.md)
- [Legacy C/C++ instrumentation](https://antithesis.com/docs/reference/sdk/cpp/old_c_instrumentation.md)
- [C++ SDK tutorial, times10 function](https://antithesis.com/docs/reference/sdk/cpp/example.md)
- [Assert macros (C++ SDK)](https://antithesis.com/docs/reference/sdk/cpp/assert.md)
- [Lifecycle functions (C++ SDK)](https://antithesis.com/docs/reference/sdk/cpp/lifecycle.md)
- [Random functions (C++ SDK)](https://antithesis.com/docs/reference/sdk/cpp/random.md)
- [JavaScript SDK](https://antithesis.com/docs/reference/sdk/javascript_sdk.md)
- [Python SDK](https://antithesis.com/docs/reference/sdk/python.md)
- [Python SDK tutorial, times10 function](https://antithesis.com/docs/reference/sdk/python/example.md)
- [Assert (Python reference)](https://antithesis.com/docs/reference/sdk/python/assert.md)
- [Lifecycle (Python reference)](https://antithesis.com/docs/reference/sdk/python/lifecycle.md)
- [Random (Python reference)](https://antithesis.com/docs/reference/sdk/python/random.md)
- [Rust SDK](https://antithesis.com/docs/reference/sdk/rust.md)
- [Rust instrumentation](https://antithesis.com/docs/reference/sdk/rust/instrumentation.md)
- [Legacy Rust instrumentation](https://antithesis.com/docs/reference/sdk/rust/legacy_instrumentation.md)
- [Rust SDK tutorial, times10 function](https://antithesis.com/docs/reference/sdk/rust/example.md)
- [Assert (Rust reference)](https://antithesis.com/docs/reference/sdk/rust/assert.md)
- [Lifecycle (Rust reference)](https://antithesis.com/docs/reference/sdk/rust/lifecycle.md)
- [Random (Rust reference)](https://antithesis.com/docs/reference/sdk/rust/random.md)
- [.NET SDK](https://antithesis.com/docs/reference/sdk/dotnet.md)
- [.NET instrumentation](https://antithesis.com/docs/reference/sdk/dotnet/instrumentation.md)
- [.NET SDK tutorial, Times10 method](https://antithesis.com/docs/reference/sdk/dotnet/example.md)
- [Assert (.NET reference)](https://antithesis.com/docs/reference/sdk/dotnet/assert.md)
- [Lifecycle (.NET reference)](https://antithesis.com/docs/reference/sdk/dotnet/lifecycle.md)
- [Random (.NET reference)](https://antithesis.com/docs/reference/sdk/dotnet/random.md)
- [Fallback SDK](https://antithesis.com/docs/reference/sdk/fallback.md)
- [Assertion functionality (Fallback SDK)](https://antithesis.com/docs/reference/sdk/fallback/assert.md)
- [Lifecycle functionality (Fallback SDK)](https://antithesis.com/docs/reference/sdk/fallback/lifecycle.md)
- [Antithesis Assertion Schema](https://antithesis.com/docs/reference/sdk/fallback/schema.md)
- [Webhooks](https://antithesis.com/docs/reference/webhook/overview.md)
- [Launching a test](https://antithesis.com/docs/reference/webhook/test_webhook.md)
- [Launching a debugging session](https://antithesis.com/docs/reference/webhook/notebook_webhook.md)
- [Webhook parameters](https://antithesis.com/docs/reference/webhook/webhook_reference.md)
- [Handling external dependencies](https://antithesis.com/docs/reference/dependencies.md)
- [Glossary](https://antithesis.com/docs/reference/glossary.md)

## FAQ

- [Product FAQs](https://antithesis.com/docs/faq/customer_faq.md): Setup checks, test frequency, mocks, logs, and test features.
- [About Antithesis POCs](https://antithesis.com/docs/faq/poc_faq.md): POC structure and suitable bugs.

## Release notes

- [Release notes](https://antithesis.com/docs/release_notes.md)

## General reliability resources

- [A distributed systems reliability glossary](https://antithesis.com/docs/resources/reliability_glossary.md): Reliability terms and deeper references.
- [Techniques for better software testing](https://antithesis.com/docs/resources/testing_techniques.md): Randomness, swarm testing, concurrency, and stronger validation.
- [Autonomous testing](https://antithesis.com/docs/resources/autonomous_testing.md): How autonomous testing relates to property-based testing and automated testing.
- [Deterministic simulation testing](https://antithesis.com/docs/resources/deterministic_simulation_testing.md): How deterministic simulation testing works and when to use it.
- [Property-based testing](https://antithesis.com/docs/resources/property_based_testing.md): How property-based testing relates to fuzzing and generative testing.
- [Catalog of reliability properties for key-value datastores](https://antithesis.com/docs/resources/kv_property_catalog.md): Safety and liveness properties for key-value datastores.
- [Catalog of reliability properties for blockchains](https://antithesis.com/docs/resources/blockchain_property_catalog.md): Safety and liveness properties for blockchains.
- [Test ACID compliance with a ring test](https://antithesis.com/docs/resources/ring_test.md): A ring-test workload for ACID graph databases.
- [Test state machine replication with a chain of blocks workload](https://antithesis.com/docs/resources/chain-of-blocks.md): A chain-of-blocks workload for state machine replication.
- [How much does an outage cost?](https://antithesis.com/docs/resources/cost_of_outages.md): A framework for outage cost estimates.
