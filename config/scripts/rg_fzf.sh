#!/usr/bin/env bash

SESSION="$1"
PATTERN="$2"

selection=$(rg --line-number --column --no-heading --color=never -e "$PATTERN" | \
  SHELL=bash fzf \
    --delimiter=':' \
    --preview='
      file={1}
      line={2}
      if command -v bat &>/dev/null; then
        bat --color=always --style=numbers,header \
            --highlight-line "$line" \
            "$file"
      else
        nl -ba "$file" | sed -n "$((line-10)),$((line+10))p"
      fi
    ' \
    --preview-window='right:60%:~3:+{2}-5' \
    --bind='ctrl-/:toggle-preview' \
    --prompt="/$PATTERN > " \
    --header='CTRL-/ to toggle preview')

if [[ -n "$selection" ]]; then
  file=$(echo "$selection" | cut -d: -f1)
  line=$(echo "$selection" | cut -d: -f2)
  col=$(echo  "$selection" | cut -d: -f3)

  row=$((line - 1))
  col=$((col - 1))

  booster exec -s "$SESSION" "o $file"
  booster exec -s "$SESSION" "goto $col $row"
fi
