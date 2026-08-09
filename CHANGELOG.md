# Changelog

## v3.0.1

- Simplified deployment to one user-facing WebUI port.
- Fixed the container HTTP listener to internal port 8080.
- Removed the `PORT` environment variable from Compose, examples and Unraid configuration.
- Added an Unraid template installer using `/mnt/cache/appdata/ashan-openrouter-manager` by default.
- Unraid now exposes only `WebUI Port`, `App Data`, `APP_MASTER_KEY` and optional `RUST_LOG`.
- Preserved the current GitHub `tsconfig.node.json` `noEmit` build fix.
- Changed GitHub Actions concurrency to use `github.ref` instead of `github.sha`.
- Removed unused QEMU and optional Docker Hub login; GHCR is the single release registry.
- Corrected GHCR image tagging for main, release tags, semantic versions and commit SHA.
