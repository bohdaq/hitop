#!/usr/bin/env bash
# The few S3 calls the backups need (PUT, GET, LIST, DELETE of objects), with curl's built-in AWS
# Signature V4 (--aws-sigv4, curl 7.75 or newer; the VM has curl already, so nothing else to
# install). Path-style URLs: works with AWS S3, Hetzner/Scaleway/OVH/Backblaze/Wasabi/R2 S3 APIs and
# MinIO. Sourced by backup.sh and restore-drill.sh.
#
# Targets: BACKUP_TARGET=s3://<bucket>/<prefix> with S3_ENDPOINT (https://...), S3_REGION,
# S3_ACCESS_KEY_ID, S3_SECRET_ACCESS_KEY; or file:///<directory> (a local or mounted directory;
# tests, or a second copy).

target_kind() { case "$BACKUP_TARGET" in s3://*) echo s3;; file://*) echo file;; *) echo "BACKUP_TARGET must be s3://bucket/prefix or file:///dir, not ${BACKUP_TARGET:-unset}" >&2; return 1;; esac; }
s3_bucket() { local p="${BACKUP_TARGET#s3://}"; echo "${p%%/*}"; }
s3_prefix() { local p="${BACKUP_TARGET#s3://}"; p="${p#*/}"; [ "$p" = "$(s3_bucket)" ] && p=""; echo "${p%/}"; }
file_dir() { echo "${BACKUP_TARGET#file://}"; }
s3_curl() {
  : "${S3_ENDPOINT:?S3_ENDPOINT is not set}" "${S3_ACCESS_KEY_ID:?S3_ACCESS_KEY_ID is not set}" "${S3_SECRET_ACCESS_KEY:?S3_SECRET_ACCESS_KEY is not set}"
  # The secret goes to curl on stdin's config, never on its command line (ps).
  printf 'user = "%s:%s"\n' "$S3_ACCESS_KEY_ID" "$S3_SECRET_ACCESS_KEY" | curl -fsS --retry 3 --retry-delay 2 --config - --aws-sigv4 "aws:amz:${S3_REGION:-us-east-1}:s3" "$@"
}
object_url() {
  local prefix; prefix="$(s3_prefix)"
  echo "${S3_ENDPOINT%/}/$(s3_bucket)/${prefix:+$prefix/}$1"
}

# put <local file> <name>
put_object() {
  case "$(target_kind)" in
    file) mkdir -p "$(file_dir)" && cp "$1" "$(file_dir)/$2.part" && mv "$(file_dir)/$2.part" "$(file_dir)/$2";;
    s3) s3_curl -o /dev/null -T "$1" "$(object_url "$2")";;
  esac
}

# get <name> <local file>
get_object() {
  case "$(target_kind)" in
    file) cp "$(file_dir)/$1" "$2";;
    s3) s3_curl -o "$2" "$(object_url "$1")";;
  esac
}

# Every object name under the target, one a line, sorted.
list_objects() {
  case "$(target_kind)" in
    file) [ -d "$(file_dir)" ] && (cd "$(file_dir)" && ls -1 | grep -v '\.part$' | sort) || true;;
    s3)
      local prefix token="" page
      prefix="$(s3_prefix)"
      while :; do
        page="$(s3_curl -G "${S3_ENDPOINT%/}/$(s3_bucket)" --data-urlencode "list-type=2" --data-urlencode "prefix=${prefix:+$prefix/}" ${token:+--data-urlencode "continuation-token=$token"})"
        echo "$page" | grep -o '<Key>[^<]*</Key>' | sed -e 's#<Key>##' -e 's#</Key>##' -e "s#^${prefix:+$prefix/}##"
        token="$(echo "$page" | grep -o '<NextContinuationToken>[^<]*' | sed 's#<NextContinuationToken>##' || true)"
        [ -n "$token" ] || break
      done | sort;;
  esac
}

delete_object() {
  case "$(target_kind)" in
    file) rm -f "$(file_dir)/$1";;
    s3) s3_curl -o /dev/null -X DELETE "$(object_url "$1")";;
  esac
}
