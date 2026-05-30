# Deployment

Status: assumed

Study Buddy Pro builds as a separate desktop app from Thuki.

Default bundle identity:

- Product name: `Study Buddy Pro`
- Bundle identifier: `com.vindepemarte.studybuddypro`
- Repository: `https://github.com/vindepemarte/study-buddy-pro`

The updater endpoint should point at the Study Buddy Pro repository before production release.

Windows beta defaults:

- Build on a Windows machine or GitHub Actions Windows runner.
- Produce an unsigned NSIS installer with `bun run build:windows`.
- Expect SmartScreen friction until a signing certificate is added.
- Treat runtime installer changes as new beta versions; stale app-local Supertonic/search folders can survive reinstall.
