#!/usr/bin/env bash
# Validate that the selected Rust test actually ran with the expected outcome.
assert_test_outcome() {
  local expected=$1 status=$2 name=$3 log=$4 marker summary
  case "$expected" in
    passed)
      [[ "$status" == 0 ]] || return 1
      marker=ok
      summary='test result: ok. 1 passed; 0 failed; 0 ignored;'
      ;;
    failed)
      [[ "$status" == 101 ]] || return 1
      marker=FAILED
      summary='test result: FAILED. 0 passed; 1 failed; 0 ignored;'
      ;;
    *) return 1 ;;
  esac
  grep -Fxq -- "test $name ... $marker" "$log" && grep -Fq -- "$summary" "$log"
}
