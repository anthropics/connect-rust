# connect-rust — Claude Code Instructions

## Pre-Commit Code Review

Before producing a commit, run **both** review agents in parallel (single
message, two Agent tool calls):

- `rust-code-reviewer` — correctness, safety, ownership/lifetimes, performance
- `rust-api-ergonomics-reviewer` — downstream-consumer perspective:
  happy-path friction, lints that fire in user crates, runtime footguns,
  doc drift, generated-code readability

The two are complementary (different lenses on the same diff) and produce
largely non-overlapping findings. Address all **Critical**, **High**, and
**Medium** findings from both. For **Low** / advisory findings, flag these
to the user to decide.

For changes that touch only internal/test/bench code with no public-API
surface, the ergonomics reviewer may be skipped.

## Regenerating Checked-In Code

If a change to `connectrpc-codegen` (or a buffa version bump) affects
generated output, regenerate before committing or CI's diff check will fail:

```bash
task generate:all
```

This rebuilds the protoc plugins from the sibling `../buffa` checkout and
regenerates `conformance/`, `examples/{eliza,multiservice}/`, and
`benches/rpc/` checked-in code.

## Local buffa Override

`task buffa:link` / `task buffa:unlink` toggle a `.cargo/config.toml`
path-override to build against the sibling `../buffa` checkout instead of
crates.io. CI does not have the override, so it always builds against the
workspace `[patch.crates-io]` git pin (or crates.io once that's dropped).
