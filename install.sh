#!/bin/sh
set -e
curl -LsSf "https://github.com/AusCyberman/dotfile-sync/releases/download/v0.3.6/dots" > /tmp/dots
chmod +x /tmp/dots
/tmp/dots $@ sync
