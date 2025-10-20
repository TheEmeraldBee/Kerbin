#!/usr/bin/env bash

SESSION="$1"
CURRENT_BUFFER="$2"

CURRENT_PATH="$CURRENT_BUFFER"
if [[ "$CURRENT_PATH" == "<scratch>" ]]; then
    CURRENT_PATH="./"
fi

PATHS=$(yazi --chooser-file=/dev/stdout "$CURRENT_PATH")

while IFS= read -r LINE; do
    if [[ -n "$LINE" ]]; then
        echo "$LINE"

        cargo run -p kerbin -- command -s "$SESSION" -c "o $LINE"
    fi
done <<< "$PATHS"
