#!/usr/bin/env bash
# Read-only PID/workspace evidence for this one live CLI session. No argv/env.
# Started before the application so the outline child is observed too.
set -u
root=reviews/2026-10-03-claude-headless/claude-live
until [[ -s "$root/app.pid" ]]; do sleep 0.05; done
app_pid=$(cat "$root/app.pid")
[[ "$app_pid" =~ ^[0-9]+$ ]] || exit 1
printf '%s app=%s observed\n' "$(date -u +%FT%TZ)" "$app_pid"
declare -A paths=() finished=()
while kill -0 "$app_pid" 2>/dev/null; do
  for children_file in /proc/"$app_pid"/task/*/children; do
    children=""
    read -r children < "$children_file" || true
    for child in $children; do
      [[ "$child" =~ ^[0-9]+$ ]] || continue
      [[ -n "${paths[$child]+set}" ]] && continue
      cwd=$(readlink "/proc/$child/cwd") || continue
      [[ "$cwd" == /tmp/cyoa-request-* ]] || continue
      paths[$child]=$cwd
      printf '%s child=%s started workspace=%s\n' "$(date -u +%FT%TZ)" "$child" "$cwd"
    done
  done
  for child in "${!paths[@]}"; do
    [[ -n "${finished[$child]+set}" ]] && continue
    if ! kill -0 "$child" 2>/dev/null && [[ ! -e "${paths[$child]}" ]]; then
      finished[$child]=yes
      printf '%s child=%s absent workspace=absent\n' "$(date -u +%FT%TZ)" "$child"
    fi
  done
  sleep 0.1
done
for child in "${!paths[@]}"; do
  if kill -0 "$child" 2>/dev/null || [[ -e "${paths[$child]}" ]]; then
    printf 'cleanup_unconfirmed child=%s workspace=%s\n' "$child" "${paths[$child]}"
    exit 1
  fi
done
printf '%s app=absent all_observed_children_and_workspaces=absent\n' "$(date -u +%FT%TZ)"
