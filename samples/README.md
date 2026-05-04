# Samples

This folder contains small, redacted test samples for the diagnostic rules.

Prefer compact examples over full user logs:

- `log_excerpt.txt`: the relevant log lines
- `expected.txt`: the expected finding type and explanation direction
- `README.md`: short notes for the sample

Real or larger anonymized logs live under `samples/real/<case-id>/` with:

- `Log.txt`
- `expected.json`

These cases are used by `tests/corpus.rs` to prevent regressions.
