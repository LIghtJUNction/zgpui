# Releasing zgpui

A release validates one exact commit before publishing its tag, crate and
GitHub Release. A retry can keep that same candidate without another version bump.

## Setup

- Add `CARGO_REGISTRY_TOKEN` as a repository secret with publishing rights for
  `zgpui`, including its first publication when needed.
- Allow the release workflow to push to `main` and create tags.
- Commit `Cargo.lock` with the package.

A missing token fails before any candidate commit. Publication always requires
the token, including a resumed run.

## First publication or a new version

Open **Actions → Release zgpui → Run workflow** and select `main`.

| Input | Use |
| --- | --- |
| `bump = none` | Publish the version already in Cargo.toml, including the initial 0.1.0 |
| `bump = patch / minor / major` | Create and push a version candidate |
| `candidate` | Leave empty for a new run |

The workflow then:

1. Records the candidate commit and version.
2. Runs core and native Linux, macOS and Windows tests against that exact SHA.
   Linux native jobs exercise OpenGL and CPU rendering separately, including
   real private-window checks and CPU sanitizer coverage.
3. Checks WebAssembly, bare-metal compilation, formatting, Clippy,
   zero dependencies and the packaged crate from that same SHA. Native builds
   also compile the packaged crate with their feature enabled, preventing a
   default-only package check from hiding missing C sources.
4. Tags the validated candidate and publishes the crate.
5. Creates the GitHub Release and attaches the source crate.

The validation workflow is also available manually for a selected commit.

## Resume a failed release

Run the workflow with `candidate` set to the original **full commit SHA**.
The candidate must belong to this repository's `main` history.
The workflow reads its existing version and ignores `bump`.

Validation runs against the fixed candidate again. An existing tag must resolve
to that commit. An existing crates.io version is reused only when its registry
checksum matches the freshly packaged candidate. An existing release asset must
also match; different bytes fail the run without being overwritten.

This supports retries after an upload completes but Cargo's index wait times
out, or after GitHub Release creation is interrupted. Missing credentials,
network errors and mismatched artifacts remain failures.

## Local native validation

`python3 scripts/check_native.py --features native --window --package` uses
the same native gates as a release. On Linux, run it again with
`--features linux-software` for the independent CPU path. Add `--offline`
when all required toolchains are already installed. Windows checks acquire
the installed MSVC environment without printing its values.
