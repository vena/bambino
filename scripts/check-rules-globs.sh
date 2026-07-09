#!/usr/bin/env bash
# Checks that every `paths:` glob in .claude/rules/*.md still matches at least
# one real file. A rule's whole mechanism is "load automatically when a
# matching file is touched" — a glob that matches nothing (typo, or the file/
# directory it named got renamed/split in a refactor) means the rule silently
# stopped loading. Nobody sees an error; the safety net is just gone. See
# CLAUDE.md and .claude/rules/wire-framing-hardware-verification.md's own
# history for why this matters more for some rules than others.
#
# Handles the two glob shapes actually used today: an exact file path, and a
# directory prefix ending in "/**". Any other glob shape is flagged as
# unsupported rather than silently assumed correct — extend this script if a
# rule ever needs one.
set -euo pipefail
cd "$(dirname "$0")/.."

status=0

for rule_file in .claude/rules/*.md; do
    [ -e "$rule_file" ] || continue
    in_paths=0
    while IFS= read -r line; do
        if [[ "$line" == "paths:" ]]; then
            in_paths=1
            continue
        fi
        if [[ $in_paths -eq 1 ]]; then
            [[ "$line" == "---" ]] && break
            if [[ "$line" =~ ^[[:space:]]*-[[:space:]]*\"(.+)\"[[:space:]]*$ ]]; then
                glob="${BASH_REMATCH[1]}"
                if [[ "$glob" == */\*\* ]]; then
                    dir="${glob%/**}"
                    if [ ! -d "$dir" ] || [ -z "$(find "$dir" -type f 2>/dev/null)" ]; then
                        echo "STALE: $rule_file — \"$glob\" matches no files (missing/empty dir: $dir)"
                        status=1
                    fi
                elif [[ "$glob" == *'*'* ]]; then
                    echo "UNSUPPORTED glob shape (extend this script): $rule_file — \"$glob\""
                    status=1
                elif [ ! -e "$glob" ]; then
                    echo "STALE: $rule_file — \"$glob\" does not exist"
                    status=1
                fi
            fi
        fi
    done < "$rule_file"
done

[ $status -eq 0 ] && echo "All .claude/rules/*.md path globs match at least one real file."
exit $status
