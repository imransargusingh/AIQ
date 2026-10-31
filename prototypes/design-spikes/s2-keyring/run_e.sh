#!/bin/bash
eval "$(gnome-keyring-daemon --start --components=secrets 2>/dev/null)"
timeout 20 ./target/debug/s2 > /tmp/aiq-s2/e.out 2>&1; echo "s2 exit $?" >> /tmp/aiq-s2/e.out
