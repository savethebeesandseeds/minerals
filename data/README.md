# Data root

This directory contains two different classes of data:

- `minerals/`, including its sample image, contains immutable,
  version-controlled legacy import seed inputs for a new checkout.
- `minerals.db`, its WAL/SHM sidecars, `backups/`, `reports/`, and generated
  images are mutable private runtime state and are intentionally ignored.

A clean checkout creates its SQLite authority from the tracked seed inputs at
startup. Never commit a live database, WAL, backup, or generated report. Back
up the complete mutable data root using the procedure in `docs/OPERATIONS.md`.
