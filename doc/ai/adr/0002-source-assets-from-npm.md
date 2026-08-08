# 0002: Source cherry from npm instead of vendored assets

Status: accepted, 2026-08-08. Supersedes the vendored-assets part of 0001.

## Context

The initial prototype vendored `assets/` copied from a local cherry
checkout. That made `cargo build` self-contained but the provenance of
the assets was a PROVENANCE text file, not a verifiable pin.

## Decision

package.json pins an exact `cherry-cljs` version; pnpm-lock.yaml records
the integrity hash. build.rs reads the modules from
`node_modules/cherry-cljs/` and fails with a hint when `pnpm install` has
not run. CI (`.github/workflows/main.yml`, modeled on cream's) builds
linux amd64/aarch64, macOS aarch64/amd64 and Windows amd64 on every
commit to main and uploads the binaries to the rolling `dev` prerelease.

## Consequences

- Reproducible: the binary is a function of pnpm-lock.yaml, Cargo.lock
  and the Rust toolchain.
- cherry-cljs 0.6.35 (current pin) predates the August 2026 repl fixes:
  a second `require` in one session throws ("s is not defined") and the
  gensym rename collision ("'seq__37' is read-only") can surface. Bump
  the pin when the next cherry release ships; no other change needed.
- Local no-network builds need a populated pnpm store.
