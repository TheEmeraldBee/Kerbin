#!/usr/bin/env bash

SESSION="$1"

FILE=$(rg --files --ignore | fzf)

if [[ -n "$FILE" ]]; then
    kerbin command -s "$SESSION" -c "o $FILE"
fi
