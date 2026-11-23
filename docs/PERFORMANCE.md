# Performance Benchmarks

Phase 13A uses Criterion benchmarks in `core/benches/phase13.rs`.

## Workloads

- `analyze/one_jar`: inspect one deterministic fixture JAR.
- `analyze/300_jars`: analyze a generated 300-JAR corpus.
- `scan/300_jars`: scan the same 300-JAR workload.
- `pack/inspect_300_mods`: import and analyze a folder containing 300 JARs.
- `port/plan_and_dry_run_forge_1_16`: run the migration planner and migration engine in dry-run mode on the Forge 1.16 source fixture.
- `remap/jar_5_mib`: remap a generated 5 MiB JAR containing a fixture class and an uncompressed payload.
- `scan/5_mib`: scan a generated JAR containing a 5 MiB source entry.

The corpus is generated from checked-in fixture JARs at benchmark setup time. Fixture creation and copying are outside the timed sections. No network access or generated binary corpus is required.

## Running

From the repository root:

```sh
cargo bench -p ummp-core --bench phase13
```

Criterion output is written under `target/criterion`. Do not compare measurements across machines without recording the environment.

## Baseline

<escape>**Pending.**</escape> The GitHub-only editing environment used for this implementation does not provide a local Rust toolchain or executable checkout, so benchmark timings have not been run. No performance numbers or targets are claimed.

Record the following when the benchmark is run on a development machine:

- CPU model and physical/logical core count
- RAM
- OS and kernel
- `rustc --version`
- benchmark command
- commit SHA and date
- Criterion estimates for every workload

## Deferred guide workloads

The benchmark targets cover all six workload categories in the guide. The 5 MiB remap fixture is generated from the checked-in class fixture and an uncompressed deterministic payload. Baseline measurements remain pending until the suite can be run on a development machine.
