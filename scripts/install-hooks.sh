#!/usr/bin/env bash
# Installs the tracked git hooks from scripts/hooks/ into .git/hooks/.
# .git/hooks/ isn't tracked by git, so this script (and the source hooks in
# scripts/hooks/) is what makes the commit gate survive a fresh clone. Run
# once after cloning: scripts/install-hooks.sh
set -euo pipefail
cd "$(dirname "$0")/.."

git_dir="$(git rev-parse --git-dir)"
mkdir -p "$git_dir/hooks"

for hook in scripts/hooks/*; do
    name="$(basename "$hook")"
    cp "$hook" "$git_dir/hooks/$name"
    chmod +x "$git_dir/hooks/$name"
    echo "Installed $name -> $git_dir/hooks/$name"
done
