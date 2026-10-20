#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

if ! gh auth status >/dev/null 2>&1; then
  gh auth login -h github.com -p https -w
fi

if [ ! -d .git ]; then
  git init -b main
fi
git add -A
if ! git diff --cached --quiet; then
  git commit -m "Build Over There GPX route-sharing MVP"
fi

if gh repo view gnotnek/over-there >/dev/null 2>&1; then
  if ! git remote get-url origin >/dev/null 2>&1; then
    git remote add origin https://github.com/gnotnek/over-there.git
  fi
  git push -u origin main
else
  gh repo create gnotnek/over-there --public --source=. --remote=origin --push \
    --description "Open source GPX cycling routes: discover, share, inspect and download rides."
fi

gh repo view gnotnek/over-there --web
