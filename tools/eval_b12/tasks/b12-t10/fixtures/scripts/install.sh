#!/usr/bin/env bash
set -euo pipefail
export PIP_INDEX_URL=https://mirror-a.local/pypi/simple
pip install -r req/requirements.txt
