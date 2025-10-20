#!/usr/bin/env bash

SESSION="$1"

FILE=$(rg --files --ignore | fzf)

if [[ -n "$FILE" ]]; then
    cargo run -p kerbin -- command -s "$SESSION" -c "o $FILE"
fi
