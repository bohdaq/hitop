# Import fixtures

Files of other tools, vendored unchanged with their licences, so the importers (`hitop-core/src/convert.rs`) are tested on
what those tools write. `cli/crates/hitop/tests/imports.rs` imports them.

| Folder | Source | License |
| --- | --- | --- |
| `insomnia/` | [Kong/insomnia](https://github.com/Kong/insomnia) `packages/insomnia-smoke-test/fixtures/*.yaml` and `packages/insomnia-inso/src/db/fixtures/insomnia-v4/insomnia_v4.json` at `cb79b8d7c1477561294e3c7ba16329563636b58b` | Apache-2.0 (`insomnia/LICENSE`) |
| `bruno/testbench/` | [usebruno/bruno](https://github.com/usebruno/bruno) some files of `packages/bruno-tests/collection` at `5291f84961b2e1d89b61c620d30b0b198344bad1` | MIT (`bruno/LICENSE`) |

`har/recording.har` is written for hitop's tests.
