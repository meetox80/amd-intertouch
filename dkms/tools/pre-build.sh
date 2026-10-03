#!/bin/bash
set -euo pipefail
top=$(cd "$(dirname "$0")/.." && pwd)
want=$(cut -c2- "$top/KERNEL_SOURCE")
have=${1:?kernelver}
want_series=${want%.*}
have_series=${have%%-*}
have_series=${have_series%.*}
if [[ $want_series != "$have_series" ]]; then
  echo "amd-intertouch: warning: sources are from kernel $want but this build is for $have; run amd-intertouch status after the next boot" >&2
fi
exit 0
