# Dependencies point inward through application use cases

The original two-crate sketch mixed game rules with adapters. The project instead
uses domain, application, presentation and infrastructure boundaries: application
owns orchestration and inward ports, infrastructure implements them, presentation
drives use cases, and the executable wires concrete implementations. Lightweight
CQRS distinguishes state-changing commands from read-only queries without adding
a message bus or separate databases. The 2026-11-03 cleanup moves canonical session
policy into application with read-only views and execution ports. Business generation
and repository capabilities live in domain; application owns technical reports and
binds cancellation/attempt types. Infrastructure owns threads, processes and storage.

Contract: [ARCH-001](../decisions/README.md#arch-001-dependencies-point-inward).
Rationale: [layer ownership](../../PLAN.md#clean-architecture-ddd-and-lightweight-cqrs).
