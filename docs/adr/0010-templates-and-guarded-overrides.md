# Use native schemas and guard arbitrary template overrides

Prompt prose and schema descriptions live in external templates while schema
structure comes from boundary types, keeping editable text separate from the shape
the adapters deserialize. Instructions, prompts and schemas come from the same
validated configuration instance. Arbitrary public overrides remain deferred until
validation of the complete effective configuration protects business invariants;
structural parsing, sample renders and snapshots alone cannot establish semantic safety.

Contracts: [PROMPTS-002](../decisions/README.md#prompts-002-external-templates-and-native-structured-output),
[PROMPTS-003](../decisions/README.md#prompts-003-arbitrary-overrides-require-business-invariant-validation).
Rationale: [schema ownership](../../PLAN.md#schema-generation).
