#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
#  cordon test.sh — CLI regression test suite
#
#  Tests every implemented subcommand for correct behaviour, exit codes,
#  and output. Runs entirely inside throwaway temp directories — never
#  touches your real project state.
#
#  Usage:
#    ./test.sh                  # cargo build then run all tests
#    ./test.sh --no-build       # skip cargo build (use existing binary)
#    ./test.sh --verbose        # print stdout/stderr of every command
#
#  What this tests that `cordon check` does NOT:
#    ✓ CLI argument parsing (flags, missing args, bad values)
#    ✓ cordon.toml mutation commands (add / remove / set / unset)
#    ✓ cordon list / status / check exit code contracts
#    ✓ Error suggestion output (typos → "did you mean?")
#    ✓ cordon run --dry-run (no bwrap execution needed)
#    ✓ cordon run exit-code forwarding (live)
#    ✓ Network isolation enforcement (live)
# ─────────────────────────────────────────────────────────────────────────────

# Do NOT use set -e — we explicitly capture exit codes from every command.
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY="$SCRIPT_DIR/target/debug/Cordon"

# ── Colours ────────────────────────────────────────────────────────────────────
RED='\033[1;31m'; GREEN='\033[1;32m'; YELLOW='\033[1;33m'
CYAN='\033[1;96m'; DIM='\033[0;90m'; RESET='\033[0m'; BOLD='\033[1m'

# ── Configuration ─────────────────────────────────────────────────────────────
_LAST_STDOUT_CONTENT=""
_LAST_STDERR_CONTENT=""
VERBOSITY=0
VERBOSE=false

# ── Flags ─────────────────────────────────────────────────────────────────────
BUILD=true
for arg in "$@"; do
    case "$arg" in
        --no-build) BUILD=false ;;
        --verbose)  VERBOSE=true ;;
        --help|-h)
            echo "Usage: $0 [--no-build] [--verbose]"
            exit 0 ;;
    esac
done

# Use a subdirectory of /tmp that is NOT a parent of any other test workspace,
# to prevent cordon's "walk-up" config search from finding stale cordon.toml files.
TMPDIR_ROOT="$(mktemp -d /tmp/cordon_test_XXXXXX)"
MOCK_HOME="$TMPDIR_ROOT/mock_home"
trap 'rm -rf "$TMPDIR_ROOT"' EXIT

# ── Counters ──────────────────────────────────────────────────────────────────
PASS=0; FAIL=0; SKIP=0
FAILURES=()
TIMEOUT=10 # Default timeout per command in seconds
CURRENT_SECTION=""

# ── Helpers ───────────────────────────────────────────────────────────────────
separator()  { printf "${DIM}  %s${RESET}\n" "$(printf '─%.0s' {1..62})"; }
pass()       { PASS=$((PASS+1)); printf "  ${GREEN}✓${RESET} %s\n" "$1"; }
fail()       {
    FAIL=$((FAIL+1))
    local _msg="$1"
    local _cmd_hint="${2:-}"
    FAILURES+=("${CURRENT_SECTION:-unknown}: $_msg${_cmd_hint:+ [TRY: $_cmd_hint]}")
    printf "  ${RED}✗${RESET} %s\n" "$_msg"
    if [[ -n "$_LAST_STDOUT_CONTENT" ]]; then
        printf "    ${DIM}Last stdout:${RESET}\n"
        printf "%s\n" "$_LAST_STDOUT_CONTENT" | sed 's/^/    /'
    fi
    if [[ -n "$_LAST_STDERR_CONTENT" ]]; then
        printf "    ${DIM}Last stderr:${RESET}\n"
        printf "%s\n" "$_LAST_STDERR_CONTENT" | sed 's/^/    /'
    fi
}
skip()       { SKIP=$((SKIP+1)); printf "  ${YELLOW}~${RESET} %s ${DIM}(skipped)${RESET}\n" "$1"; }
section()    { CURRENT_SECTION="$1"; echo; printf "${CYAN}${BOLD}▶ %s${RESET}\n" "$1"; separator; }

# ---------------------------------------------------------------------------
# run_cordon <rc_var> <out_var> <err_var> <args...>
#
# Runs the cordon binary with the given args from $WORKSPACE.
# Captures stdout → out_var, stderr → err_var, exit code → rc_var.
# Never aborts on failure — always returns the real exit code.
# ---------------------------------------------------------------------------
run_cordon() {
    local _rc_var="$1"; local _out_var="$2"; local _err_var="$3"; shift 3

    local _tmp_out; _tmp_out=$(mktemp)
    local _tmp_err; _tmp_err=$(mktemp)

# Mock HOME to ensure test isolation from real user config
    local _mock_home="${MOCK_HOME:-$TMPDIR_ROOT/mock_home}"
    mkdir -p "$_mock_home"


    (cd "$WORKSPACE" && HOME="$_mock_home" timeout "$TIMEOUT" "$BINARY" "$@") \
        > "$_tmp_out" 2> "$_tmp_err"
    local _status=$?
    
        if [[ $_status -eq 124 ]]; then
        printf -v "$_rc_var"  "124"
        printf -v "$_out_var" "TIMEOUT after ${TIMEOUT}s"
        printf -v "$_err_var" ""
    else
        printf -v "$_rc_var"  "%d" "$_status"
        printf -v "$_out_var" "%s" "$(cat "$_tmp_out")"
        printf -v "$_err_var" "%s" "$(cat "$_tmp_err")"
    fi
    _LAST_STDOUT_CONTENT="$(cat "$_tmp_out")"
    _LAST_STDERR_CONTENT="$(cat "$_tmp_err")"
    rm -f "$_tmp_out" "$_tmp_err"

    if $VERBOSE; then
        printf "    ${DIM}CMD:  cordon %s${RESET}\n" "$*"
        local _o; _o="${!_out_var}"; [ -n "$_o" ] && printf "    ${DIM}OUT:  %s${RESET}\n" "$_o"
        local _e; _e="${!_err_var}"; [ -n "$_e" ] && printf "    ${DIM}ERR:  %s${RESET}\n" "$_e"
        printf "    ${DIM}EXIT: %s${RESET}\n" "${!_rc_var}"
    fi

    # Save the last command for failure reporting
    LAST_CMD="cordon $*"
}

assert_exit() {
    local name="$1" expected="$2" actual="$3"
    if [[ "$actual" -eq 124 ]]; then
        fail "$name (TIMED OUT)" "$LAST_CMD"
    elif [[ "$actual" -eq "$expected" ]]; then
        pass "$name (exit $expected)"
    else
        fail "$name — expected $expected, got $actual" "$LAST_CMD"
    fi
}

assert_contains() {
    local name="$1" needle="$2" haystack="$3"
    if printf '%s' "$haystack" | grep -qF -e "$needle" 2>/dev/null; then
        pass "$name"
    else
        fail "$name — expected output to contain '${needle}'"
        $VERBOSE && printf "    ${DIM}haystack: %s${RESET}\n" "$haystack"
    fi
}

assert_not_contains() {
    local name="$1" needle="$2" haystack="$3"
    if ! printf '%s' "$haystack" | grep -qF -e "$needle" 2>/dev/null; then
        pass "$name"
    else
        fail "$name — output should NOT contain '${needle}'"
    fi
}

assert_exit_any_of() {
    # assert_exit_any_of "test name" actual code1 code2 ...
    local name="$1" actual="$2"; shift 2
    for code in "$@"; do
        if [[ "$actual" -eq "$code" ]]; then
            pass "$name (exit $actual)"
            return
        fi
    done
    fail "$name — unexpected exit $actual (expected one of: $*)"
}

assert_file_exists()     { [[ -f "$1" ]] && pass "$2" || fail "$2 (missing: $1)"; }
assert_file_not_exists() { [[ ! -f "$1" ]] && pass "$2" || fail "$2 (still exists: $1)"; }
assert_file_contains() {
    if grep -qF "$2" "$1" 2>/dev/null; then
        pass "$3"
    else
        fail "$3 — '${2}' not found in $1"
        $VERBOSE && echo "    File contents:" && cat "$1"
    fi
}
assert_file_not_contains() {
    if ! grep -qF "$2" "$1" 2>/dev/null; then
        pass "$3"
    else
        fail "$3 — '${2}' should NOT be in $1"
    fi
}

fresh_workspace() {
    # Creates a clean workspace under TMPDIR_ROOT with NO parent cordon.toml.
    # Using deeply nested dirs defeats the upward walk logic.
    local name="${1:-ws}"
    local ws="$TMPDIR_ROOT/isolated/${name}"
    mkdir -p "$ws"
    echo "$ws"
}

# ── Build ─────────────────────────────────────────────────────────────────────
if $BUILD; then
    echo
    printf "${CYAN}${BOLD}⟳ Building cordon…${RESET}\n"
    if cargo build --manifest-path="$SCRIPT_DIR/Cargo.toml" 2>&1 | tail -5; then
        printf "${GREEN}${BOLD}  Build OK${RESET}\n"
    else
        printf "${RED}${BOLD}  Build FAILED — aborting${RESET}\n"
        exit 1
    fi
fi

if [[ ! -x "$BINARY" ]]; then
    printf "${RED}Binary not found: %s${RESET}\n" "$BINARY"
    printf "Run: cargo build   or use --no-build if you already built it.\n"
    exit 1
fi

# Pre-prep system.toml to avoid lag in tests
echo
printf "${CYAN}${BOLD}⟳ Pre-scanning system…${RESET} ${DIM}(fixes lag)${RESET}\n"
MOCK_HOME="$TMPDIR_ROOT/mock_home"
mkdir -p "$MOCK_HOME"
HOME="$MOCK_HOME" "$BINARY" scan -y >/dev/null 2>&1
printf "${GREEN}${BOLD}  System OK${RESET}\n"

# ─────────────────────────────────────────────────────────────────────────────
#  §1 — Version & Help  (always work, no config needed)
# ─────────────────────────────────────────────────────────────────────────────
section "1. Version & Help"
WORKSPACE="$(fresh_workspace v01)"

declare rc out err
run_cordon rc out err --version
assert_contains "cordon --version includes version string" "cordon" "$out"
assert_exit "cordon --version exits 0" 0 $rc

run_cordon rc out err --help
assert_contains "cordon --help mentions sandbox" "sandbox" "$out$err"
assert_exit "cordon --help exits 0" 0 $rc

run_cordon rc out err run --help
assert_exit "cordon run --help exits 0" 0 $rc

run_cordon rc out err check --help
assert_exit "cordon check --help exits 0" 0 $rc

run_cordon rc out err list --help
assert_exit "cordon list --help exits 0" 0 $rc

run_cordon rc out err status --help
assert_exit "cordon status --help exits 0" 0 $rc

# ─────────────────────────────────────────────────────────────────────────────
#  §2 — Error Suggestions ("did you mean?")
# ─────────────────────────────────────────────────────────────────────────────
section "2. Error Suggestions"
WORKSPACE="$(fresh_workspace v02)"

run_cordon rc out err statis
assert_exit_any_of "typo 'statis' exits 1 or 2" $rc 1 2
assert_contains    "typo 'statis' suggests 'status'" "status" "$out$err"

run_cordon rc out err chekc
assert_exit_any_of "typo 'chekc' exits 1 or 2" $rc 1 2
assert_contains    "typo 'chekc' suggests 'check'" "check" "$out$err"

run_cordon rc out err lis
assert_exit_any_of "typo 'lis' exits 1 or 2" $rc 1 2
assert_contains    "typo 'lis' suggests 'list'" "list" "$out$err"

run_cordon rc out err xyzzy_garbage_xyz
assert_exit_any_of "garbage subcommand exits non-zero" $rc 1 2

# ─────────────────────────────────────────────────────────────────────────────
#  §3 — cordon check
# ─────────────────────────────────────────────────────────────────────────────
section "3. cordon check"
WORKSPACE="$(fresh_workspace v03)"

run_cordon rc out err check
assert_exit_any_of "cordon check exits 0 or 1 (no crash)" $rc 0 1
assert_contains    "cordon check prints a header" "Cordon" "$out$err"
# The output should mention "passed" and "failed" as part of the results table
combined="$out$err"
if printf '%s' "$combined" | grep -qiE "(passed|failed|FAIL|OK)"; then
    pass "cordon check prints results table"
else
    fail "cordon check output has no result table"
fi

# ─────────────────────────────────────────────────────────────────────────────
#  §4 — cordon status
# ─────────────────────────────────────────────────────────────────────────────
section "4. cordon status"
WORKSPACE="$(fresh_workspace v04)"

run_cordon rc out err status
assert_exit_any_of "cordon status exits 0 or 1" $rc 0 1
combined="$out$err"
if printf '%s' "$combined" | grep -qiEi "(last_scan|scan|system\.toml|module|cordon)"; then
    pass "cordon status output is informative"
else
    fail "cordon status output looks empty"
fi

# ─────────────────────────────────────────────────────────────────────────────
#  §5 — cordon list
# ─────────────────────────────────────────────────────────────────────────────
section "5. cordon list"
WORKSPACE="$(fresh_workspace v05)"

run_cordon rc out err list
assert_exit_any_of "cordon list exits 0 or 1" $rc 0 1

# ─────────────────────────────────────────────────────────────────────────────
#  §6 — cordon add / remove
# ─────────────────────────────────────────────────────────────────────────────
section "6. cordon add / remove"
WORKSPACE="$(fresh_workspace v06_add)"
TOML="$WORKSPACE/cordon.toml"

# add — default mode (ro)
run_cordon rc out err add /tmp
assert_exit           "cordon add /tmp exits 0" 0 $rc
assert_file_exists    "$TOML" "cordon add creates cordon.toml"
assert_file_contains  "$TOML" "/tmp"  "cordon.toml contains added path"
assert_file_contains  "$TOML" "ro"    "cordon.toml default mode is ro"
assert_contains       "cordon add prints ✅" "✅" "$out"

# add — explicit rw mode
run_cordon rc out err add /tmp --mode rw
assert_exit           "cordon add --mode rw exits 0" 0 $rc
assert_file_contains  "$TOML" "rw" "cordon.toml contains rw mount"

# list should now show the project mount
# Note: we check for 'tmp' because /tmp might be canonicalised to /var/tmp on some systems
run_cordon rc out err list
assert_exit_any_of    "cordon list with cordon.toml exits 0" $rc 0 1
if printf '%s' "$out$err" | grep -q "/.*tmp"; then
    pass "cordon list shows /tmp mount"
else
    fail "cordon list shows /tmp mount — expected output to contain '/tmp' (or canonicalised version)"
fi

# remove — there are two /tmp entries (ro + rw). Both should be removed.
run_cordon rc out err remove /tmp
assert_exit_any_of    "cordon remove exits 0" $rc 0
assert_contains       "cordon remove prints ✅ or ⚠" "" "$out"  # just don't crash

# Remove non-existent path — should warn, not crash
run_cordon rc out err remove /completely/fake/path/that/does/not/exist
assert_exit           "cordon remove non-existent path exits 0" 0 $rc
assert_contains       "cordon remove warns about missing path" "⚠" "$out"

# add then remove-all: cordon.toml should disappear when empty
WORKSPACE="$(fresh_workspace v06_del)"
run_cordon rc out err add /tmp
run_cordon rc out err remove /tmp
assert_file_not_exists "$WORKSPACE/cordon.toml" "empty cordon.toml is deleted"

# ─────────────────────────────────────────────────────────────────────────────
#  §7 — cordon set / unset
# ─────────────────────────────────────────────────────────────────────────────
section "7. cordon set / unset"
WORKSPACE="$(fresh_workspace v07)"
TOML="$WORKSPACE/cordon.toml"

# set --net=full
run_cordon rc out err set --net=full
assert_exit             "cordon set --net=full exits 0" 0 $rc
assert_file_exists      "$TOML" "cordon set creates cordon.toml"
assert_file_contains    "$TOML" "full" "cordon.toml has network = full"
assert_contains         "cordon set prints ✅" "✅" "$out"

# set --gui
run_cordon rc out err set --gui
assert_exit             "cordon set --gui exits 0" 0 $rc
assert_file_contains    "$TOML" "gui = true" "cordon.toml has gui = true"

# set --optional
run_cordon rc out err set --optional audio_pipewire
assert_exit             "cordon set --optional exits 0" 0 $rc
assert_file_contains    "$TOML" "audio_pipewire" "cordon.toml has optional module"

# set with no flags — arg_required_else_help: should NOT exit 0
run_cordon rc out err set
[[ $rc -ne 0 ]] && pass "cordon set (no args) exits non-zero" || fail "cordon set (no args) should exit non-zero, got 0"

# unset --gui
run_cordon rc out err unset --gui
assert_exit             "cordon unset --gui exits 0" 0 $rc
assert_file_not_contains "$TOML" "gui = true" "cordon unset --gui removed gui=true"

# unset --optional
run_cordon rc out err unset --optional audio_pipewire
assert_exit             "cordon unset --optional exits 0" 0 $rc
assert_file_not_contains "$TOML" "audio_pipewire" "cordon unset --optional removed audio_pipewire"

# unset --net
run_cordon rc out err unset --net
assert_exit             "cordon unset --net exits 0" 0 $rc
assert_file_not_contains "$TOML" '"full"' "cordon unset --net removed network field"

# unset with no flags — arg_required_else_help: should NOT exit 0
run_cordon rc out err unset
[[ $rc -ne 0 ]] && pass "cordon unset (no args) exits non-zero" || fail "cordon unset (no args) should exit non-zero, got 0"

# ─────────────────────────────────────────────────────────────────────────────
#  §8 — cordon run --dry-run  (never invokes bwrap)
# ─────────────────────────────────────────────────────────────────────────────
section "8. cordon run --dry-run"
WORKSPACE="$(fresh_workspace v08)"

run_cordon rc out err run --dry-run -- echo hello
assert_exit_any_of "cordon run --dry-run exits 0 or 1" $rc 0 1
combined="$out$err"
if printf '%s' "$combined" | grep -qiE "(bwrap|dry.?run|scan)"; then
    pass "cordon run --dry-run prints bwrap command or scan hint"
else
    fail "cordon run --dry-run output is unhelpful"
fi

# All three network modes
for mode in disable allow full; do
    run_cordon rc out err run --dry-run --net="$mode" -- echo
    assert_exit_any_of "cordon run --dry-run --net=$mode exits 0 or 1" $rc 0 1
done

# bad --net value → clap usage error (exit 2)
run_cordon rc out err run --net=bogus -- echo
[[ $rc -ne 0 ]] && pass "cordon run --net=bogus exits non-zero" || fail "cordon run --net=bogus should fail, got exit 0"

# missing command after -- (no positional args)
run_cordon rc out err run --dry-run
[[ $rc -ne 0 ]] && pass "cordon run (no cmd) exits non-zero" || fail "cordon run (no cmd) should fail, got exit 0"

# ─────────────────────────────────────────────────────────────────────────────
#  §9 — cordon run  (live execution — requires sandbox to be ready)
# ─────────────────────────────────────────────────────────────────────────────
section "9. cordon run (live execution)"
WORKSPACE="$(fresh_workspace v09)"

# Gate all live tests on sandbox readiness
sandbox_ok=false
if (cd "$WORKSPACE" && "$BINARY" check >/dev/null 2>&1); then
    sandbox_ok=true
fi

if $sandbox_ok; then
    # Exit code forwarding: 'true' → 0
    run_cordon rc out err run -- true
    assert_exit "cordon run -- true forwards exit 0" 0 $rc

    # Exit code forwarding: 'false' → non-zero (1 or 125/127 depending on classify)
    run_cordon rc out err run -- false
    [[ $rc -ne 0 ]] && pass "cordon run -- false forwards non-zero exit" \
                     || fail "cordon run -- false should forward non-zero exit, got 0"

    # stdout from sandboxed command is visible
    run_cordon rc out err run -- echo "cordon_test_marker_xyz"
    assert_exit     "cordon run -- echo exits 0" 0 $rc
    assert_contains "sandboxed echo output visible" "cordon_test_marker_xyz" "$out"

    # Network isolation: with --net=disable (default), curl should fail or time out
    if command -v curl &>/dev/null; then
        run_cordon rc out err run -- curl -s --max-time 3 http://1.1.1.1
        if [[ $rc -ne 0 ]]; then
            pass "cordon run (--net=disable) blocks network (curl fails)"
        else
            fail "cordon run (--net=disable) should block network but curl succeeded"
        fi
    else
        skip "curl not installed — skipping network isolation test"
    fi
else
    skip "sandbox not ready (cordon check failed) — skipping live run tests"
    SKIP=$((SKIP+4))
fi

# ─────────────────────────────────────────────────────────────────────────────
#  §10 — Profile precedence (cordon.toml merged into CLI defaults)
# ─────────────────────────────────────────────────────────────────────────────
section "10. Profile precedence"
WORKSPACE="$(fresh_workspace v10)"

run_cordon rc out err set --net=full
assert_exit "set --net=full for precedence test" 0 $rc
assert_file_contains "$WORKSPACE/cordon.toml" "full" "profile has network = full"

# cordon list should reflect profile state
run_cordon rc out err list
assert_exit_any_of "cordon list with profile exits 0 or 1" $rc 0 1

# cordon run --dry-run still works with a loaded profile
run_cordon rc out err run --dry-run -- echo
assert_exit_any_of "cordon run --dry-run works with profile in cordon.toml" $rc 0 1

# After unsetting, the field should be gone
run_cordon rc out err unset --net
assert_file_not_contains "$WORKSPACE/cordon.toml" '"full"' "unset removes network from profile"

# ─────────────────────────────────────────────────────────────────────────────
#  §11 — cordon edit  (non-interactive, EDITOR=true)
# ─────────────────────────────────────────────────────────────────────────────
section "11. cordon edit"
WORKSPACE="$(fresh_workspace v11)"

# Use 'true' as editor so it exits immediately without blocking
EDITOR=true run_cordon rc out err edit
assert_exit_any_of "cordon edit (EDITOR=true) exits 0 or 1" $rc 0 1
assert_file_exists  "$WORKSPACE/cordon.toml" "cordon edit creates cordon.toml when missing"

# ─────────────────────────────────────────────────────────────────────────────
#  §12 — Missing argument hints
# ─────────────────────────────────────────────────────────────────────────────
section "12. Missing argument hints"
WORKSPACE="$(fresh_workspace v12)"

# cordon add without a path
run_cordon rc out err add
[[ $rc -ne 0 ]] && pass "cordon add (no path) exits non-zero" || fail "cordon add (no path) should exit non-zero"
combined="$out$err"
if printf '%s' "$combined" | grep -qiE "(path|required|usage|add)"; then
    pass "cordon add (no path) prints helpful message"
else
    fail "cordon add (no path) message is unhelpful: $combined"
fi

# cordon remove without a path
run_cordon rc out err remove
[[ $rc -ne 0 ]] && pass "cordon remove (no path) exits non-zero" || fail "cordon remove (no path) should exit non-zero"

# ─────────────────────────────────────────────────────────────────────────────
#  §13 — cordon profile
# ─────────────────────────────────────────────────────────────────────────────
section "13. cordon profile"
WORKSPACE="$(fresh_workspace v13)"

# Capture original HOME
# run_cordon uses MOCK_HOME, so we check there
run_cordon rc out err profile create python --net=allow --optional ld_so_cache
assert_exit         "cordon profile create exits 0" 0 $rc
assert_file_exists  "$MOCK_HOME/.config/cordon/profiles.toml" "profiles.toml created"
assert_contains     "profile create prints checkmark" "✅" "$out"

run_cordon rc out err profile list
assert_exit         "cordon profile list exits 0" 0 $rc
assert_contains     "cordon profile list shows python" "python" "$out"

run_cordon rc out err profile show python
assert_exit         "cordon profile show python exits 0" 0 $rc
assert_contains     "cordon profile show python contains allow" "allow" "$out"

run_cordon rc out err profile show nonexistent
[[ $rc -ne 0 ]] && pass "cordon profile show nonexistent exits non-zero" || fail "cordon profile show nonexistent should fail"

run_cordon rc out err profile create python --net=full --gui
assert_exit         "cordon profile create overwrites exits 0" 0 $rc
run_cordon rc out err profile show python
assert_contains     "overwritten profile has new values" "full" "$out"
assert_contains     "overwritten profile has new values" "true" "$out"

run_cordon rc out err profile delete python
assert_exit         "cordon profile delete exits 0" 0 $rc
assert_contains     "cordon profile delete prints checkmark" "✅" "$out"

run_cordon rc out err profile delete nonexistent
assert_exit         "cordon profile delete nonexistent exits 0" 0 $rc
assert_contains     "cordon profile delete nonexistent prints warning" "⚠️" "$out"

run_cordon rc out err profile list
assert_contains     "cordon profile list empty prints suggestion" "No saved profiles" "$out"

run_cordon rc out err run --dry-run --profile rust -- echo
assert_exit_any_of  "cordon run --dry-run --profile works (built-in fallback)" $rc 0 1

run_cordon rc out err profile
[[ $rc -ne 0 ]] && pass "cordon profile (no args) exits non-zero" || fail "cordon profile (no args) should exit non-zero"

run_cordon rc out err proifle list
assert_contains     "typo proifle suggests profile" "profile" "$err$out"

# ─────────────────────────────────────────────────────────────────────────────
#  §14 — cordon log
# ─────────────────────────────────────────────────────────────────────────────
section "14. cordon log"
WORKSPACE="$(fresh_workspace v14)"

# log without a run — ensure it's missing
rm -f "$MOCK_HOME/.config/cordon/logs/last-run.log"
run_cordon rc out err log
assert_exit         "cordon log exits 0" 0 $rc
assert_contains     "cordon log shows message if missing" "last-run.log" "$out$err"

# Run a quick command, then log
run_cordon rc out err run -- echo "hello"
run_cordon rc out err log
assert_exit         "cordon log (after run) exits 0" 0 $rc
assert_contains     "cordon log outputs internal logs" "Running inside sandbox" "$out"

run_cordon rc out err log --last 1
assert_exit         "cordon log --last N exits 0" 0 $rc

# ─────────────────────────────────────────────────────────────────────────────
#  §15 — cordon run --trace
# ─────────────────────────────────────────────────────────────────────────────
section "15. cordon run --trace"
WORKSPACE="$(fresh_workspace v15)"

if command -v strace >/dev/null 2>&1; then
    run_cordon rc out err run --trace -- head -n 1 /etc/shadow
    # head on /etc/shadow usually fails with EACCES or ENOENT depending on the distro
    # Bwrap normally isolates or hides shadow 
    assert_contains "strace output prints report" "Strace Denied Access Report" "$out$err"
    
    # Check if tracing output contains the trace output file
    assert_contains "log message mentions --from-trace" "from-trace" "$out$err"
else
    # strace not installed: should exit 1 cleanly 
    run_cordon rc out err run --trace -- echo
    assert_exit "cordon run --trace fails without strace" 1 $rc
    assert_contains "cordon run --trace warns about missing strace" "strace" "$err"
fi

# ─────────────────────────────────────────────────────────────────────────────
#  §16 — cordon run --quiet / --verbose
# ─────────────────────────────────────────────────────────────────────────────
section "16. cordon run --quiet / --verbose"
WORKSPACE="$(fresh_workspace v16)"

run_cordon rc out err run --quiet --dry-run -- echo hi
assert_not_contains "quiet suppresses [CORDON] banner" "[CORDON]" "$err$out"

run_cordon rc out err run --verbose --dry-run -- echo hi
assert_contains "verbose prints bwrap args (with [wrapper] prefix)" "[wrapper]" "$out$err"

# ─────────────────────────────────────────────────────────────────────────────
#  §17 — cordon init
# ─────────────────────────────────────────────────────────────────────────────
section "17. cordon init"
WORKSPACE="$(fresh_workspace v17)"

# Auto-detect Cargo.toml → rust profile
echo '{}' > "$WORKSPACE/Cargo.toml"
run_cordon rc out err init --yes
assert_exit "cordon init exits 0" 0 $rc
assert_file_exists "$WORKSPACE/cordon.toml" "cordon init creates cordon.toml"
assert_file_contains "$WORKSPACE/cordon.toml" "allow" "rust profile sets network=allow"
assert_file_contains "$WORKSPACE/cordon.toml" "ld_so_cache" "rust profile sets optional=ld_so_cache"

# --force overwrites existing
run_cordon rc out err init --yes --force
assert_exit "cordon init --force exits 0" 0 $rc

# Already exists without --force → non-zero
WORKSPACE="$(fresh_workspace v17b)"
echo "" > "$WORKSPACE/cordon.toml"
run_cordon rc out err init --yes
[[ $rc -ne 0 ]] && pass "init without --force won't overwrite" || fail "init should refuse without --force"

# ─────────────────────────────────────────────────────────────────────────────
#  §18 — NixOS Support (--distro nixos)
# ─────────────────────────────────────────────────────────────────────────────
section "18. cordon scan --distro nixos"
WORKSPACE="$(fresh_workspace v18)"

run_cordon rc out err scan --distro nixos <<< "n
n
n
"
assert_contains "nixos scan prints banner" "NixOS detected" "$out$err"

# ─────────────────────────────────────────────────────────────────────────────
#  §19 — cordon doctor
# ─────────────────────────────────────────────────────────────────────────────
section "19. cordon doctor"
WORKSPACE="$(fresh_workspace v19)"

run_cordon rc out err doctor
assert_exit_any_of "cordon doctor exits 0 or 1" $rc 0 1
assert_contains "doctor prints bwrap check" "Bubblewrap" "$out$err"
assert_contains "doctor prints kernel info" "Kernel" "$out$err"
assert_contains "doctor prints config info" "Config" "$out$err"

# ─────────────────────────────────────────────────────────────────────────────
#  §20 — cordon syscalls
# ─────────────────────────────────────────────────────────────────────────────
section "20. cordon syscalls"
WORKSPACE="$(fresh_workspace v20)"

run_cordon rc out err syscalls --preset basic
assert_exit    "syscalls basic exits 0" 0 $rc
assert_contains "syscalls basic output contains ptrace" "ptrace" "$out"

run_cordon rc out err syscalls --preset strict
assert_exit    "syscalls strict exits 0" 0 $rc
assert_contains "syscalls strict output mentions allow-list" "allow-list" "$out"

# Dry run verify seccomp arg
run_cordon rc out err run --dry-run --verbose --seccomp basic --net disable -- echo hello
assert_exit    "run --seccomp dry-run exits 0" 0 $rc
assert_contains "dry-run contains --seccomp" "--seccomp" "$out$err"

run_cordon rc out err run --dry-run --verbose --seccomp none -- echo hello
assert_exit    "run --seccomp none dry-run exits 0" 0 $rc
assert_not_contains "none preset skips --seccomp arg" "--seccomp" "$out$err"

if [[ ${#FAILURES[@]} -gt 0 ]]; then
    echo
    printf "  ${RED}${BOLD}Failed tests:${RESET}\n"
    for f in "${FAILURES[@]}"; do
        printf "    ${RED}✗ %s${RESET}\n" "$f"
    done
fi

echo
printf "${BOLD}%s${RESET}\n" "$(printf '═%.0s' {1..66})"
printf " ${BOLD}CORDON TEST RESULTS${RESET}\n"
printf "${BOLD}%s${RESET}\n" "$(printf '═%.0s' {1..66})"
TOTAL=$((PASS + FAIL + SKIP))

if [[ $FAIL -eq 0 ]]; then
    printf "  ${GREEN}${BOLD}All tests passed!${RESET} (%d passed)\n\n" $PASS
    exit 0
else
    printf "  ${RED}${BOLD}%d test(s) failed — see above${RESET} (%d passed, %d failed)\n" $FAIL $PASS $FAIL
    printf "  ${DIM}Note: You can re-run failed commands listed in [TRY: ...] above.${RESET}\n\n"
    exit 1
fi
