#!/usr/bin/env bash
set -euo pipefail
ethornell_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
python3 "$ethornell_root/tools/build_retrom_candidate.py" "${1:?absolute empty output directory required}"
