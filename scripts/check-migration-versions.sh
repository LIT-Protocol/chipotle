#!/usr/bin/env bash
# Fail if two migration files in the same migrations/ directory share a version
# prefix (the leading numeric token before the first underscore).
#
# sqlx derives a migration's version from that prefix, so two files with the
# same prefix collide: the _sqlx_migrations row for that version can only match
# one checksum, and the other trips "migration <v> was previously applied but
# has been modified" on every boot. This happened in production when
# 20260623000001_gas_funder.sql and 20260623000001_enterprise_billing.sql were
# merged independently. This check makes that a CI failure instead.
set -euo pipefail

status=0

# Every sqlx migrations dir in the repo (excluding build/vendor output).
while IFS= read -r dir; do
  # Collect duplicate version prefixes among *.sql files in this dir.
  dupes=$(
    find "$dir" -maxdepth 1 -name '*.sql' -exec basename {} \; \
      | sed -E 's/^([0-9]+)_.*/\1/' \
      | sort \
      | uniq -d
  )
  if [ -n "$dupes" ]; then
    status=1
    while IFS= read -r v; do
      echo "ERROR: duplicate migration version '$v' in $dir:"
      find "$dir" -maxdepth 1 -name "${v}_*.sql" -exec echo "    {}" \;
    done <<< "$dupes"
  fi
done < <(find . -type d -name migrations -not -path '*/node_modules/*' -not -path '*/target/*' | sort)

if [ "$status" -eq 0 ]; then
  echo "OK: no duplicate migration versions"
fi
exit "$status"
