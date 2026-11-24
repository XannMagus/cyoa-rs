# Project contracts govern source parity

The port deliberately departs from Calibre in several places; tests against Python
alone previously missed violations of those decisions. Explicit user decisions
and recorded project contracts therefore govern behavior, with Calibre serving as
the reference wherever no deviation is recorded. Behavioral changes require the
existing decision, rationale and regression-update procedure rather than silently
restoring source behavior.

Authority and change procedure: [project contracts](../decisions/README.md#precedence-and-change-procedure).
The [required-test registry](../decisions/required-tests.json) records executable
obligations; historical plans and evidence do not establish current coverage.
