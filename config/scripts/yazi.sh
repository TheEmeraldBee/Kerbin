#!/usr/bin/env bash

SESSION="$1"
CURRENT_BUFFER="$2"

CURRENT_PATH="$CURRENT_BUFFER"
if [[ ! -e "$CURRENT_PATH" ]]; then
    CURRENT_PATH="./"
fi

PATHS=$(yazi --chooser-file=/dev/stdout "$CURRENT_PATH")

while IFS= read -r LINE; do
    if [[ -n "$LINE" ]]; then
        if [[ -f "$LINE" ]]; then
            echo "$LINE"
            booster exec -s "$SESSION" "o $LINE"
        else
            booster exec -s "$SESSION" "echo --level critical ['$LINE is not a file']"
            echo "Not a file: $LINE"
        fi
    fi
done <<< "$PATHS"
