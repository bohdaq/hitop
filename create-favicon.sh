#!/bin/bash
# Every HITOP icon (site favicons included) is rendered from brand/logo.svg by brand/export.sh.
exec "$(dirname "$0")/brand/export.sh" "$@"
