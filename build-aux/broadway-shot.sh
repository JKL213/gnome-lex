#!/bin/sh
# Broadway-Screenshot: zwei Aufnahmen, die größere (vollständige) gewinnt.
out=$HOME/.cache/gnome-lex/shots/$1.png
best=0
for i in 1 2 3; do
  tmp=$HOME/.cache/gnome-lex/shots/.tmp$i.png
  timeout 60 chromium-browser --headless --no-sandbox --disable-gpu --hide-scrollbars --window-size=1200,800 --virtual-time-budget=8000 --screenshot="$tmp" http://127.0.0.1:8087/ >/dev/null 2>&1
  size=$(stat -c %s "$tmp" 2>/dev/null || echo 0)
  if [ "$size" -gt "$best" ]; then best=$size; cp "$tmp" "$out"; fi
  [ "$size" -gt 20000 ] && [ "$i" -ge 2 ] && break
  sleep 1
done
ls -la "$out"
