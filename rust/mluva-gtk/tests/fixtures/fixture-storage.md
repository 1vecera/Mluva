# Frozen fixture storage

These observations still come from unchanged Mluva v1.6.0 at `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. The [53-line test loader](../support/fixture_states.rs) stores invariant object fields once in `common_observations` per group and, for transaction fixtures, `common_observation` per transaction. It reconstructs complete expected states before the existing assertions. Arrays/nulls remain literal values; conflicting leaves, ambiguous flat/grouped rows and unread groups are rejected. There are no overrides or references to previous states. No production code, test case, assertion, seed, action or source metadata is removed.

| Fixture | Complete states | Original lines → stored lines | Original bytes → stored bytes |
| --- | ---: | ---: | ---: |
| Capture preferences | 132 | 42,624 → 9,160 | 1,592,545 → 287,452 |
| Provider/setup pages | 290 | 41,391 → 19,869 | 1,434,374 → 630,298 |
| Conversation page | 86 | 31,904 → 29,546 | 831,876 → 744,743 |
| Capture controls | 77 | 9,646 → 8,194 | 238,084 → 195,637 |
| Review controller | 102 | 12,939 → 8,122 | 436,873 → 278,173 |
| History page | 64 | 12,228 → 11,246 | 467,396 → 423,679 |
| Meeting page | 44 | 7,649 → 6,549 | 331,149 → 278,223 |
| **Total** | **795** | **158,381 → 92,686** | **5,332,297 → 2,838,205** |

Native `serde_json::Value` equality against the original committed documents and identical canonical bytes establish preservation independently of the application. The originals are recoverable from parent commits `785a526` (first two files) and `c58a579` (remaining five); ignored scratch also retains their exact bytes. Each fixture's existing evidence page records its current stored hash. The canonical hashes below match both the original and expanded documents, including all actions/metadata and the conversation fixture's sixteen pure Questions cases.

| Fixture | Original raw SHA-256 | Original = expanded canonical SHA-256 |
| --- | --- | --- |
| Capture preferences | `b5eb6ec9c79238c6714250e885d9b2cff1a7f817da51d551195235b985b20715` | `6156831c4ef44f3ff79727a534fd8a37754af469beabc636bd15b8d5b116ef27` |
| Provider/setup pages | `56ecf65cc543ccd3bf574365efe455c3dea3d9afbf95891a2c5da184d5792638` | `ca74c63eb072e3cf9f97c5f273fb74ac93be203f3c2b5da62d9b5aa7b2b6ddb5` |
| Conversation page | `283659166867e00a34ff3b6e5dda0cc2e229e6db0abe37cf38d06d55b664cca9` | `b238816483e45c9e4a80ad81d0f615a90eac2b0f8616ee912e547c3afbeca0d2` |
| Capture controls | `19215f9e0e5fe8d6dbd9837ff8c687376da868c5b181d82f031f7ffa2f4ee989` | `e73040a1102d732107c84f7ede137ef1cde4a7b453dab6988797ec20485441b0` |
| Review controller | `2b9935ad3bd01adb68efc84b5b223d1fbf2849113c746f827acea2046e33176c` | `2e91e8dd5c44ea87870da8fc03ea4a89a23b861dd468efc7f0ef8f9327a8bfac` |
| History page | `107946693bbaac0da392b0854299b22c651ef3f5fdb46662a4de97f13205f92a` | `edc0b2b8d8fdc62914e05e48591d38ffe130fcca232d8ddb78228feb76bd64d7` |
| Meeting page | `b2350180541809be4ef285159a95d725d1c7adc67efb71f0704aa9aad9ddab22` | `d0e0358e889af16fc9efa43140774210e2b96e5993622a0989fd73b6c992627c` |

The second batch's native equality log is `tmp/native-page-size-preservation.log`; raw/canonical/expanded documents are in `tmp/pr-size-second/` and `tmp/pr-size-original/`. All seven explicit private GTK owners pass after compaction, including existing persistence failures, dropped-owner faults, scrolling/stability and immediate-save/teardown checks. The five newly factored owners also pass before the edit. The initial combined baseline wrongly supplied UTC to History; its only differences were three Prague timestamp labels. Replaying with its documented `Europe/Prague` timezone resolves that invocation failure without changing code or expectations. Logs remain in `tmp/native-page-size-baseline.log`, `tmp/native-page-size-baseline-rest.log` and `tmp/native-page-size-factored.log`.

Actual `make test` passes 139 tests, zero failures and 68 deliberately ignored environment checks across 98 suites, both strict Clippy configurations, formatting and generated consistency (`tmp/native-page-size-final-native.log`). The ordinary Questions and language-choice consumers still pass with unchanged data. No Python code changes in this batch; its 571-test/Ruff result at `c58a579` remains applicable. Installed app/widget stay v1.6.0. This data cleanup does not establish additional workflow, physical-device or performance acceptance; the full Rust goal remains open.
