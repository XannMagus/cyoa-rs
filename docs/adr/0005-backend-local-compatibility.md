# Adapt compatibility limits within each backend

Live calls revealed that the vendors accept different schema shapes. Shared DTOs,
schema generation and prompt rendering retain the maximal backend-neutral form;
each backend adapts its own copy in a separate compatibility file, so implementing
one backend cannot require changing the other's shared inputs. A dynamic plugin
registry was deferred because two backends need only simple registration; shared
vendor-neutral transport plumbing remains reusable without interpreting protocols.

Contract and observed adaptation examples:
[ARCH-003](../decisions/README.md#arch-003-backend-specific-tolerance-lives-in-that-backends-own-adapter-file).
