#!/usr/bin/env bash
# scripts/build_agent.sh - Package a Nudge program into a standalone agent binary.
#
# Supports two runtime flavors:
#   1. Full (default): live-provider capable, ureq/TLS included (~2.1MB).
#   2. Replay-only (--replay-only): lightweight, no network/TLS (~866KB), offline replay only.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

usage() {
    cat <<EOF
Usage: $(basename "$0") <prog.ndg> [OUT] [--fn ENTRY] [--replay-only]

Packages a Nudge program (.ndg) into a standalone native executable.

Arguments:
  <prog.ndg>       Path to the Nudge source file (required)
  [OUT]            Output binary path (default: file stem of program)

Options:
  --fn ENTRY       Entry function name (default: main)
  --replay-only    Package with lightweight offline replay runtime (~866KB)
                   instead of full live-provider runtime (~2.1MB)
  -h, --help       Show this help message

Environment variables:
  NUDGEC              Path to nudgec compiler binary (default: target/release/nudgec)
  NUDGE_AGENT_RUNTIME Path to prebuilt runtime binary (overrides automatic selection)
EOF
}

PROG=""
OUT=""
ENTRY=""
REPLAY_ONLY=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --replay-only)
            REPLAY_ONLY=1
            shift
            ;;
        --fn)
            if [[ $# -lt 2 || "$2" == --* ]]; then
                echo "error: --fn requires an entry function name" >&2
                exit 1
            fi
            ENTRY="$2"
            shift 2
            ;;
        --fn=*)
            ENTRY="${1#*=}"
            if [[ -z "$ENTRY" ]]; then
                echo "error: --fn requires an entry function name" >&2
                exit 1
            fi
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        -*)
            echo "error: unknown option '$1'" >&2
            usage >&2
            exit 1
            ;;
        *)
            if [[ -z "$PROG" ]]; then
                PROG="$1"
            elif [[ -z "$OUT" ]]; then
                OUT="$1"
            else
                echo "error: unexpected positional argument '$1'" >&2
                usage >&2
                exit 1
            fi
            shift
            ;;
    esac
done

if [[ -z "$PROG" ]]; then
    echo "error: missing required program argument <prog.ndg>" >&2
    usage >&2
    exit 1
fi

if [[ ! -f "$PROG" ]]; then
    echo "error: source file not found: '$PROG'" >&2
    exit 1
fi

PROG_FILE="$(basename "$PROG")"
STEM="${PROG_FILE%.ndg}"
if [[ -z "$OUT" ]]; then
    OUT="$STEM"
fi

# Ensure output path doesn't clobber the input source file
ABS_PROG="$(readlink -f "$PROG" 2>/dev/null || realpath "$PROG" 2>/dev/null || echo "$PROG")"
ABS_OUT="$(readlink -f "$OUT" 2>/dev/null || realpath "$OUT" 2>/dev/null || echo "$OUT")"
if [[ "$ABS_PROG" == "$ABS_OUT" ]]; then
    echo "error: output path '$OUT' would overwrite source file '$PROG'" >&2
    exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo is required to build missing components but was not found in PATH" >&2
    exit 1
fi

echo "=========================================================="
echo " Nudge Standalone Agent Packaging"
echo "=========================================================="
echo " Program:        $PROG"
echo " Output:         $OUT"
if [[ -n "$ENTRY" ]]; then
    echo " Entry function: $ENTRY"
else
    echo " Entry function: main (default)"
fi
if [[ "$REPLAY_ONLY" -eq 1 ]]; then
    echo " Flavor:         replay-only (small, ~866KB, offline)"
else
    echo " Flavor:         full (live-capable, ~2.1MB, ureq/TLS)"
fi
echo "=========================================================="

# 1. Compiler check / build
NUDGEC="${NUDGEC:-$REPO_ROOT/target/release/nudgec}"
echo "==> [1/3] Checking nudgec compiler..."
if [[ ! -x "$NUDGEC" ]]; then
    echo "Compiler not found at '$NUDGEC'. Building (cargo build --release -p nudgec)..."
    cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" -p nudgec
else
    # Check if compiler source files are newer than the binary
    if [[ -n "$(find "$REPO_ROOT/crates/nudgec" "$REPO_ROOT/Cargo.toml" -type f -newer "$NUDGEC" 2>/dev/null)" ]]; then
        echo "Compiler sources changed. Rebuilding (cargo build --release -p nudgec)..."
        cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" -p nudgec
    else
        echo "Found up-to-date compiler: $NUDGEC"
    fi
fi

if [[ ! -x "$NUDGEC" ]]; then
    echo "error: nudgec compiler binary not found or not executable at '$NUDGEC'" >&2
    exit 1
fi

# 2. Runtime check / build
FULL_RUNTIME="$REPO_ROOT/target/release/nudge-agent"
REPLAY_RUNTIME="$REPO_ROOT/target/release/nudge-agent-replay"

if [[ -n "${NUDGE_AGENT_RUNTIME:-}" ]]; then
    echo "==> [2/3] Using explicitly configured NUDGE_AGENT_RUNTIME: $NUDGE_AGENT_RUNTIME"
    SELECTED_RUNTIME="$NUDGE_AGENT_RUNTIME"
    if [[ ! -x "$SELECTED_RUNTIME" ]]; then
        echo "error: specified NUDGE_AGENT_RUNTIME '$SELECTED_RUNTIME' is not executable" >&2
        exit 1
    fi
elif [[ "$REPLAY_ONLY" -eq 1 ]]; then
    echo "==> [2/3] Preparing replay-only runtime variant..."
    NEED_BUILD_REPLAY=0
    if [[ ! -x "$REPLAY_RUNTIME" ]]; then
        NEED_BUILD_REPLAY=1
    elif [[ -n "$(find "$REPO_ROOT/crates/nudge-agent" "$REPO_ROOT/crates/nudge-runtime" "$REPO_ROOT/Cargo.toml" -type f -newer "$REPLAY_RUNTIME" 2>/dev/null)" ]]; then
        NEED_BUILD_REPLAY=1
    fi

    if [[ "$NEED_BUILD_REPLAY" -eq 1 ]]; then
        echo "Building replay-only runtime (cargo build --release -p nudge-agent --no-default-features)..."
        cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" -p nudge-agent --no-default-features

        echo "Copying replay-only binary to sidecar: $REPLAY_RUNTIME..."
        mkdir -p "$(dirname "$REPLAY_RUNTIME")"
        cp -p "$FULL_RUNTIME" "$REPLAY_RUNTIME"

        echo "Restoring default full runtime in target/release/nudge-agent..."
        cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" -p nudge-agent
    else
        echo "Found up-to-date replay-only runtime at: $REPLAY_RUNTIME"
    fi

    # Ensure default runtime wasn't left in replay-only state
    if [[ -x "$FULL_RUNTIME" ]] && ! grep -q -F "ureq" "$FULL_RUNTIME" 2>/dev/null; then
        echo "Default runtime is currently non-default flavor. Rebuilding default runtime..."
        cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" -p nudge-agent
    fi

    SELECTED_RUNTIME="$REPLAY_RUNTIME"
else
    echo "==> [2/3] Preparing full (default) runtime variant..."
    NEED_BUILD_FULL=0
    if [[ ! -x "$FULL_RUNTIME" ]]; then
        NEED_BUILD_FULL=1
    elif ! grep -q -F "ureq" "$FULL_RUNTIME" 2>/dev/null; then
        echo "Default runtime is missing live-provider features. Rebuilding..."
        NEED_BUILD_FULL=1
    elif [[ -n "$(find "$REPO_ROOT/crates/nudge-agent" "$REPO_ROOT/crates/nudge-runtime" "$REPO_ROOT/Cargo.toml" -type f -newer "$FULL_RUNTIME" 2>/dev/null)" ]]; then
        NEED_BUILD_FULL=1
    fi

    if [[ "$NEED_BUILD_FULL" -eq 1 ]]; then
        echo "Building full runtime (cargo build --release -p nudge-agent)..."
        cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml" -p nudge-agent
    else
        echo "Found up-to-date full runtime at: $FULL_RUNTIME"
    fi

    SELECTED_RUNTIME="$FULL_RUNTIME"
fi

if [[ ! -x "$SELECTED_RUNTIME" ]]; then
    echo "error: selected runtime binary not found or not executable at '$SELECTED_RUNTIME'" >&2
    exit 1
fi

# 3. Package standalone agent
echo "==> [3/3] Packaging standalone agent..."
BUILD_ARGS=("$NUDGEC" "build" "$PROG" "--standalone" "-o" "$OUT")
if [[ -n "$ENTRY" ]]; then
    BUILD_ARGS+=("--fn" "$ENTRY")
fi

echo "Executing: NUDGE_AGENT_RUNTIME=\"$SELECTED_RUNTIME\" ${BUILD_ARGS[*]}"
NUDGE_AGENT_RUNTIME="$SELECTED_RUNTIME" "${BUILD_ARGS[@]}"

# 4. Verify output binary
if [[ ! -f "$OUT" ]]; then
    echo "error: expected output binary '$OUT' was not created" >&2
    exit 1
fi

if [[ ! -x "$OUT" ]]; then
    echo "error: output binary '$OUT' was created but is not executable" >&2
    exit 1
fi

OUT_SIZE="$(wc -c < "$OUT" | tr -d ' ')"
OUT_SIZE_KB="$((OUT_SIZE / 1024))"

echo "=========================================================="
echo " Standalone agent packaged successfully!"
echo " Location: $OUT"
echo " Size:     $OUT_SIZE bytes (~${OUT_SIZE_KB} KB)"
if [[ "$REPLAY_ONLY" -eq 1 ]]; then
    echo " Mode:     replay-only (requires recorded trace: NUDGE_REPLAY=trace.jsonl $OUT)"
else
    echo " Mode:     full (live LLM provider capable, offline replay supported)"
fi
echo "=========================================================="
