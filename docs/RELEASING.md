# Release checklist (no automatic publication)

Jirust is a personal open-source project. Public source availability and local packaging do not authorize release uploads, history rewriting, organization transfers or visibility changes. Those require explicit owner approval.

## Requirements

- Rust 1.90+, make, Python 3.11+ and git.
- `cargo install cargo-audit --version 0.22.2 --locked`
- `cargo install cargo-about --version 0.9.2 --locked --features cli`
- Gitleaks 8.30.1, verified against the official release checksum.

## Gates

1. Synthetic-only regression tests, `make check`, `make build`, demo read-only/no-cache diagnostic, RustSec audit with warnings denied and secret scans must pass. No real Jira mutation is a test.
2. Run `cargo fetch --locked` then `make notices` after any lockfile change (generation is offline/deterministic); review selected license alternatives and source inventory. `make notices-check` must pass. MPL-2.0 source obligations are file-level, not blanket relicensing of Jirust. Review requirements for your exact distribution; generated notices are not legal advice.
3. Review the whole Git history, proposed tracked files and GitHub Actions logs for secrets/company data; a clean current tree alone is insufficient. Do not bundle raw audit logs, dotenv/config files, caches, screenshots, local tools or real-account fixtures.
4. Use the support matrix and read-only checklist. Document any unverified site/board/platform; do not convert CI success into a live compatibility claim.
5. Review the diff and security review evidence, push to the owner's repository and verify native/minimum-version/security CI. Verify private vulnerability reporting and exact distribution eligibility. Uploading release artifacts is a separate explicit owner decision; never infer authorization from a public repository or a local package.

## Dependency maintenance

The `Refresh dependencies` workflow runs daily and supports manual dispatch on the default branch. It updates compatible direct/transitive versions, respects the package's minimum Rust version during resolution, fetches sources, and regenerates both third-party inventories. Changes go to one `automation/dependencies` PR for manual review. No release or automatic merge is performed.

Repository setup: under **Settings → Actions → General → Workflow permissions**, enable **Allow GitHub Actions to create and approve pull requests** to permit PR creation with `GITHUB_TOKEN`. The workflow never approves PRs; its write permissions are scoped to the maintenance job. No PAT or Jira credential is needed. If this option is disabled by repository/organization policy, the PR-creation step will fail visibly; the owner must open the pushed branch's PR manually. The existing weekly audit still runs regardless.

Bot-token pushes and PR creation do not trigger normal push/PR workflows, so maintenance explicitly dispatches `Check` on the update branch. Review its Linux/macOS, Rust 1.90, audit and inventory results before merging. The workflow does not bypass branch protection. Repeated runs reuse the branch/PR and preserve review commits without force-pushes. If merging the default branch conflicts, resolve the conflict on the maintenance branch and rerun the refresh workflow; no conflict is silently discarded.

For a manual targeted fix:

```sh
cargo update -p <crate> --precise <patched-version>
cargo fetch --locked
make notices
make notices-check
make audit
make check
make build
```

When a direct dependency's minimum safe version changes, update its requirement in `Cargo.toml` too. Newly published advisories or incompatible upgrades still require timely owner attention; automation is not a promise that future security checks cannot fail.

## Local bundle

From a clean, committed tree matching the binary version:

```sh
make package
```

This builds the native binary, verifies notices, and creates a local archive under ignored `dist/` with the binary, MIT license, third-party license texts/source inventory, security/support documents, exact committed project source and checksum-verified original source archives for MPL-2.0 components. The source archives are included unchanged; local configuration and untracked files are excluded. A SHA256SUMS file identifies the archive. Verify it with `sha256sum -c SHA256SUMS` (Linux) or `shasum -a 256 -c SHA256SUMS` (macOS).

The script does not upload an artifact or change GitHub settings. Checksums establish byte identity, not a publisher signature; release signing/attestation is a separate optional policy. Build on each supported native OS/architecture rather than labeling a Linux artifact as macOS-compatible. Retain third-party source availability and notices alongside every distributed binary.
