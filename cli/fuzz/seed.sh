#!/bin/sh
# Builds the starting corpora in fuzz/corpus/<target> (not in git) from the
# repository's own fixtures, then runs one target for a while:
#   cli/fuzz/seed.sh                      # corpora only
#   cli/fuzz/seed.sh openapi 60           # and run `openapi` for 60 seconds
# Needs a nightly toolchain (FUZZ_TOOLCHAIN, default `nightly`) and cargo-fuzz
# (`cargo install cargo-fuzz`). Crashes land in fuzz/artifacts/<target>/.
set -eu
fuzz=$(cd "$(dirname "$0")" && pwd)
cli=$(dirname "$fuzz")
corpus="$fuzz/corpus"
mkdir -p "$corpus/collection_yaml" "$corpus/postman" "$corpus/openapi"

# collection_yaml: hitop.yaml, then "\0<path>\n<text>" for every other file.
folder_seed() { # <folder> <out>
  dir=$1
  {
    cat "$dir/hitop.yaml"
    (cd "$dir" && find . -type f \( -name '*.yaml' -o -name '*.yml' \) ! -path ./hitop.yaml | sort) | while read -r f; do
      printf '\0%s\n' "${f#./}"
      cat "$dir/$f"
    done
  } > "$2"
}
n=0
for dir in "$cli/crates/hitop-core/tests/golden/every-field/folder" "$cli/fixtures/sample" "$cli/../spec/compat/v1"/*/folder; do
  [ -f "$dir/hitop.yaml" ] || continue
  n=$((n + 1))
  folder_seed "$dir" "$corpus/collection_yaml/seed-$n"
done
for f in "$cli"/crates/hitop-core/tests/golden/every-field/folder/requests/*.yaml; do
  n=$((n + 1))
  printf 'format: 1\nname: x\n\0requests/a.yaml\n' | cat - "$f" > "$corpus/collection_yaml/seed-$n"
done

# postman: the vendored collections, environments and globals.
n=0
for f in "$cli"/fixtures/postman/*/*.json "$cli"/fixtures/launch-gate/*.json; do
  n=$((n + 1)); cp "$f" "$corpus/postman/seed-$n"
done

# openapi: a flags byte (0: folders per tag, defaults), then the spec. The two
# large specs are left out: they slow every mutation down.
printf '\000' | cat - "$cli/fixtures/openapi/petstore/openapi.yaml" > "$corpus/openapi/seed-petstore"
printf '\017' | cat - "$cli/fixtures/openapi/petstore/openapi.yaml" > "$corpus/openapi/seed-petstore-flags"
for f in "$fuzz"/seeds/openapi/*; do
  [ -f "$f" ] && cp "$f" "$corpus/openapi/"
done

if [ $# -ge 1 ]; then
  target=$1
  seconds=${2:-60}
  toolchain=${FUZZ_TOOLCHAIN:-nightly}
  # The host's own target: a cargo-fuzz built for musl (cargo binstall) would default to musl,
  # which the sanitizers don't support.
  host=$(rustc "+$toolchain" -vV | sed -n 's/^host: //p')
  cd "$cli"
  exec cargo "+$toolchain" fuzz run -O --target "$host" "$target" -- -max_total_time="$seconds" -timeout=25 -rss_limit_mb=2048 -print_final_stats=1
fi
