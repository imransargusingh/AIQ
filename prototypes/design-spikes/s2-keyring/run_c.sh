#!/bin/bash
eval "$(printf 'spikepw' | gnome-keyring-daemon --unlock --components=secrets 2>/dev/null)"
timeout 20 ./target/debug/s2 > /tmp/aiq-s2/c.out 2>&1; echo "s2 exit $?" >> /tmp/aiq-s2/c.out
