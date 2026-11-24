# Use Unicode lowercase matching and readable generated IDs

The port accepts Unicode lowercase as its matching approximation rather than full
Python casefold: names such as “Straße” and “STRASSE” remain distinct. Generated
IDs also use Rust's alphanumeric character classification, retaining alphabetic
combining marks for more readable word slugs instead of adding a Unicode-category
dependency to reproduce Python. These are deliberate identity policies; existing
assigned IDs persist, and changing matching requires an identity/migration assessment.

Contract: [TEXT-002](../decisions/README.md#text-002-unicode-lowercase-is-the-matching-policy).
