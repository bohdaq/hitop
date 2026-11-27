#!/usr/bin/env bash
# The desktop app as its own plugin child (src/plugins.rs, `--hitop-plugin-run`): one hook in, one answer
# out, before Tauri starts anything (so no display is needed); a plugin that grows without end is stopped
# by the child's own limits. Usage: plugin-child-check.sh <hitop-desktop binary>
set -euo pipefail
bin="$1"
# `timeout` isn't on macOS: perl's alarm ends the command with SIGALRM (exit 142).
manifest='/* hitop-plugin {\"name\": \"up\", \"version\": \"1.0.0\", \"hooks\": [\"value\"]} */\n'
input="{\"source\": \"${manifest}function value(t) { return t.toUpperCase(); }\", \"hook\": \"value\", \"args\": [\"hi\"]}"
out=$(printf '%s' "$input" | perl -e 'alarm shift; exec @ARGV' 30 env -i "$bin" --hitop-plugin-run)
if [ "$out" != '{"ok":"HI"}' ]; then
  echo "the desktop's plugin child answered: $out" >&2
  exit 1
fi
echo "plugin child: $out"
# Memory: 'a'.repeat(1e9) is refused by the engine or the child's cap, never answered; the parent's 5 s
# limit isn't here, so the alarm stands in for it and must not be what ends it.
input="{\"source\": \"${manifest}function value() { let s = 'a'; for (let i = 0; i < 40; i++) s = s + s; return s; }\", \"hook\": \"value\", \"args\": []}"
set +e
out=$(printf '%s' "$input" | perl -e 'alarm shift; exec @ARGV' 60 env -i "$bin" --hitop-plugin-run)
code=$?
set -e
if [ "$code" = 142 ] || [[ "$out" == '{"ok"'* ]]; then
  echo "a plugin that grows without end wasn't stopped (exit $code): ${out:0:200}" >&2
  exit 1
fi
echo "growing plugin stopped (exit $code): ${out:0:200}"
