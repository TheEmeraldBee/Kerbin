#!/usr/bin/env bash
SESSION="$1"

# Format: filepath:line:col:severity:message
# --with-nth reorders display to: message severity filepath
# {1},{2},{3} still refer to filepath, line, col in the original data

selection=$(printf '%s\n' "${@:2}" | SHELL=bash fzf \
  --delimiter=':' \
  --with-nth='5..,4,1' \
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
  --prompt='Diagnostics > ' \
  --header='CTRL-/ to toggle preview')

if [[ -n "$selection" ]]; then
  file=$(echo "$selection" | cut -d: -f1)
  line=$(echo "$selection" | cut -d: -f2)
  col=$(echo  "$selection" | cut -d: -f3)

  booster exec -s "$SESSION" "lsp-goto-location $file:$line:$col"
fi
