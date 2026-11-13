#!/bin/sh
# Lists the uncovered lines and branches of js/client/src/ipc*.js for the given test files.
cd "$(dirname "$0")/../../js/client" || exit 1
node --test --test-concurrency=1 --test-timeout=30000 --experimental-test-coverage \
  --test-coverage-include='src/ipc*' --test-reporter=spec --test-reporter-destination=/tmp/ipc-spec.log \
  --test-reporter=lcov --test-reporter-destination=/tmp/ipc.lcov "$@" >/dev/null 2>&1
grep -E "^ℹ (tests|pass|fail)|^✖" /tmp/ipc-spec.log | head -20
awk -F'[:,]' '/^SF/{f=$2; sub(/.*\//,"",f)} /^DA/ && $3=="0"{l[f]=l[f]" "$2} /^BRDA/ && ($5=="0"||$5=="-"){b[f]=b[f]" "$2} /^FNDA/ && $2=="0"{fn[f]=fn[f]" "$3}
END{for(k in l)print k" lines:"l[k]; for(k in b)print k" branches:"b[k]; for(k in fn)print k" functions:"fn[k]}' /tmp/ipc.lcov | sort
