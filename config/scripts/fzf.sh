#!/usr/bin/env bash

SESSION="$1"

FILE=$(rg --files --ignore | SHELL=bash fzf \
  --preview='
    file={1}
    if command -v bat &>/dev/null; then
      bat --color=always --style=numbers,header \
          "$file"
    else
      nl -ba "$file" | sed -n "$((line-10)),$((line+10))p"
    fi
  ' \
  --preview-window='right:60%:~3:+{2}-5' \
  --bind='ctrl-/:toggle-preview' \
  --prompt='Go to > ' \
  --header='CTRL-/ to toggle preview')

if [[ -n "$FILE" ]]; then
    booster exec -s "$SESSION" "o $FILE"
fi
