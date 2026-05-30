# Code Rules

Status: decided

- Keep local-first privacy as the default.
- Add features as connected backend, config, UI, persistence, and tests rather than isolated panels.
- Prefer additive SQLite migrations.
- Do not depend on the sibling `supertonic` checkout at runtime.
- Keep Study Buddy Pro separate from installed Thuki data.
- Voice startup may delegate first-run venv/model setup to Supertonic's native manager; keep runtime status observable through setup/health checks and logs.
- Do not fake Windows parity with macOS-only stubs.
- Do not run sidecar services from read-only installer resources; copy bundled runtimes into app-local data first.
