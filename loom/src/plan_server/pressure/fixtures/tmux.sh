#!/bin/sh
d=$(dirname "$0")
n=$(cat "$d/panes" 2>/dev/null || echo 1)
i=0
while [ "$i" -lt "$n" ]; do
  echo "plan-pressure:0.$i: [80x24] %$i"
  i=$((i + 1))
done
