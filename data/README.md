# Data root

This directory contains two different classes of data:

- `minerals/`, including its sample image, contains immutable,
  version-controlled legacy import seed inputs for a new checkout.
- `minerals.db`, its WAL/SHM sidecars, `backups/`, `reports/`, `pilots/`, and
  generated images are mutable private runtime state and are intentionally
  ignored. `pilots/` contains content-addressed source bytes, retrieval
  receipts, and review artifacts for bounded technical pilots; none of it is a
  public catalog input.

A clean checkout creates its SQLite authority from the tracked seed inputs at
startup. Never commit a live database, WAL, backup, or generated report. Back
up the complete mutable data root using the procedure in `docs/OPERATIONS.md`.

The intentionally public browser database lives separately under
`public-catalog/`. It is rebuilt by the exporter with a strict public-only
schema; it is not a copy of this operational data root.
