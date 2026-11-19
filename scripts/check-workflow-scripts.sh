#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Check the versioned workflow scripts in .claude/workflows/*.js
# (.agents/rules/parallel-agent-work.md, "Running a change").
#
#   scripts/check-workflow-scripts.sh [--workflows DIR] [--rules DIR]
#   scripts/check-workflow-scripts.sh --self-test
#
# For each script it requires `export const meta = {` as the first statement,
# with `name` and `description`; wraps the body in an async function (so the
# harness's top-level `await` and `return` parse) and runs `node --check` on
# the wrapper; requires every literal `phase('<title>')` to be a title in
# meta.phases; and refuses personal data and push credentials (home and
# session paths, SSH key names, `ssh -i`, GIT_SSH_COMMAND, `gh auth`, commit
# trailers, e-mail addresses). Prompts point to rule sections instead of
# copying them: any run of 12 consecutive words (case and punctuation
# ignored) that also appears in a rule file fails, and every title of a
# citation written `<rule>.md "<Section>", "<Section>" (note), and "<Section>"`
# must name a heading that exists in that rule file.
# Every script has a stub-agent harness beside it, `<name>.test.mjs`, run as
# `node <name>.test.mjs <name>.js`; a missing or failing harness fails.
#
# With no script present it passes and says so. Without node it fails.
# LUSHTEXT_NODE overrides the node binary (the self-test uses it).

set -euo pipefail
export LC_ALL=C

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)

check_dir() {
    local workflows=$1 rules=$2
    local node=${LUSHTEXT_NODE:-node}
    local scripts=()
    if [ -d "$workflows" ]; then
        mapfile -t scripts < <(find "$workflows" -maxdepth 1 -type f -name '*.js' | sort)
    fi
    if [ "${#scripts[@]}" -eq 0 ]; then
        echo "check-workflow-scripts: no workflow scripts under $workflows; nothing to check"
        return 0
    fi
    if ! command -v "$node" >/dev/null 2>&1; then
        echo "check-workflow-scripts: node not found ($node); it is required to syntax-check workflow scripts" >&2
        return 1
    fi
    local failed=0 script wrapper tmpdir
    tmpdir=$(mktemp -d)
    for script in "${scripts[@]}"; do
        wrapper="$tmpdir/$(basename "$script" .js).mjs"
        if ! python3 -I - "$script" "$rules" "$wrapper" <<'PY'
import re, sys
from pathlib import Path

script, rules_dir, wrapper = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
text = script.read_text(encoding="utf-8")
lines = text.splitlines()
errors = []

def fail(line_no, message):
    errors.append(f"{script}:{line_no}: {message}")

# 1. meta first, with name and description.
first = next((i for i, line in enumerate(lines)
              if line.strip() and not line.strip().startswith("//")), None)
if first is None or not lines[first].startswith("export const meta = {"):
    fail((first or 0) + 1, "the first statement must be `export const meta = {`")
    meta_text = ""
else:
    end = next((i for i in range(first, len(lines)) if lines[i].rstrip() == "}"), None)
    if end is None:
        fail(first + 1, "meta has no closing `}` at the start of a line")
        meta_text = ""
    else:
        meta_text = "\n".join(lines[first:end + 1])
        for key in ("name", "description"):
            if not re.search(rf"^\s*{key}\s*:", meta_text, re.MULTILINE):
                fail(first + 1, f"meta has no `{key}`")

# 2. wrapper for node --check.
body = text.replace("export const meta = {", "const meta = {", 1)
wrapper.write_text(
    "async function __workflow(args, agent, parallel, pipeline, phase, log, budget, workflow) {\n"
    + body + "\n}\n", encoding="utf-8")

# 3. phase titles.
titles = set(re.findall(r"\btitle\s*:\s*['\"`]([^'\"`]+)['\"`]", meta_text))
for number, line in enumerate(lines, 1):
    for title in re.findall(r"\bphase\(\s*['\"]([^'\"]+)['\"]\s*\)", line):
        if title not in titles:
            fail(number, f"phase('{title}') is not a title in meta.phases")

# 4. personal data and credentials.
patterns = [
    (r"/var/home/", "a home path"),
    (r"/home/[a-z]", "a home path"),
    (r"/tmp/claude-", "a session scratch path"),
    (r"\bid_[a-z0-9_]+", "an SSH key name"),
    (r"ssh -i", "an SSH identity option"),
    (r"GIT_SSH_COMMAND", "a push credential setting"),
    (r"gh auth", "a gh account command"),
    (r"Co-Authored-By:", "a commit trailer"),
    (r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}", "an e-mail address"),
]
for number, line in enumerate(lines, 1):
    for pattern, what in patterns:
        if re.search(pattern, line):
            fail(number, f"{what} (`{pattern}`); take it from args.local")

# 5. copied rule text: 12-word runs.
def words(chunk):
    return re.findall(r"[a-z0-9]+", chunk.lower())

RUN = 12
rule_runs = {}
rule_files = sorted(rules_dir.glob("*.md"))
for rule in rule_files:
    tokens = words(rule.read_text(encoding="utf-8"))
    for i in range(len(tokens) - RUN + 1):
        rule_runs.setdefault(tuple(tokens[i:i + RUN]), rule.name)
positions = []
for number, line in enumerate(lines, 1):
    for token in words(line):
        positions.append((token, number))
reported = set()
for i in range(len(positions) - RUN + 1):
    run = tuple(token for token, _ in positions[i:i + RUN])
    source = rule_runs.get(run)
    if source and (source, positions[i][1]) not in reported:
        reported.add((source, positions[i][1]))
        fail(positions[i][1], f"copies a 12-word run of .agents/rules/{source}: \"{' '.join(run)}\"; cite the section instead")

# 6. rule section citations.
headings = {}
for rule in rule_files:
    headings[rule.name] = {re.sub(r"^#+\s*", "", line).strip()
                           for line in rule.read_text(encoding="utf-8").splitlines()
                           if line.startswith("#")}
# A citation is a rule file name followed by a list of quoted titles:
# `name.md "A", "B" (a note), and "C"`. Every title of the list is checked;
# the list ends at the first text that is not a separator, a parenthetical or
# a quoted title.
TITLE = r'\\?"([^"\\]+)\\?"'
CITATION = re.compile(
    r"([A-Za-z0-9_-]+\.md),?\s+(" + TITLE + r"(?:\s*\([^()]*\))?"
    r"(?:\s*,\s*(?:and\s+)?" + TITLE + r"(?:\s*\([^()]*\))?)*)")
for number, line in enumerate(lines, 1):
    for match in CITATION.finditer(line):
        name = match.group(1)
        if name not in headings:
            continue
        titles = re.sub(r"\([^()]*\)", "", match.group(2))
        for section in re.findall(TITLE, titles):
            if section not in headings[name]:
                fail(number, f"cites {name} \"{section}\", which is not a heading of .agents/rules/{name}")

for error in errors:
    print(error, file=sys.stderr)
sys.exit(1 if errors else 0)
PY
        then
            failed=1
            continue
        fi
        if ! "$node" --check "$wrapper" 2>"$tmpdir/node.err"; then
            echo "$script: node --check failed:" >&2
            sed 's/^/  /' "$tmpdir/node.err" >&2
            failed=1
            continue
        fi
        local harness="${script%.js}.test.mjs"
        if [ ! -f "$harness" ]; then
            echo "$script: no stub-agent harness $(basename "$harness") beside it" >&2
            failed=1
        elif ! timeout 120 "$node" "$harness" "$script" >"$tmpdir/harness.out" 2>&1; then
            echo "$script: harness $(basename "$harness") failed:" >&2
            sed 's/^/  /' "$tmpdir/harness.out" >&2
            failed=1
        fi
    done
    rm -rf "$tmpdir"
    if [ "$failed" -ne 0 ]; then
        echo "check-workflow-scripts: FAILED" >&2
        return 1
    fi
    echo "check-workflow-scripts: ${#scripts[@]} workflow script(s) pass"
}

self_test_fail() {
    echo "check-workflow-scripts self-test FAILED: $*" >&2
    exit 1
}

self_test() {
    SELF_TEST_TMP=$(mktemp -d)
    trap 'rm -rf "$SELF_TEST_TMP"' EXIT INT TERM
    local tmp=$SELF_TEST_TMP
    local wf="$tmp/workflows" rules="$tmp/rules" out
    mkdir -p "$wf" "$rules"
    cat >"$rules/sample.md" <<'EOF'
# Sample Rule

## Waiting

A subagent or workflow stage waits in the foreground and repeats a bounded call until the condition holds, never ending its turn to wait.
EOF
    out=$(check_dir "$wf" "$rules") || self_test_fail "an empty directory failed"
    case "$out" in *"nothing to check"*) ;; *) self_test_fail "an empty directory did not say so" ;; esac

    good() {
        cat <<'EOF'
export const meta = {
  name: 'fixture',
  description: 'A fixture workflow',
  phases: [{ title: 'One' }, { title: 'Two' }],
}

phase('One')
const r = await agent(`Follow sample.md "Waiting" and stop.`, { label: 'x' })
if (!r) return { stopped: true }
phase('Two')
return { r }
EOF
    }
    good >"$wf/a.js"
    expect_fail_early() {
        local err
        if err=$(check_dir "$wf" "$rules" 2>&1); then self_test_fail "$1 passed"; fi
        case "$err" in *"$2"*) ;; *) self_test_fail "$1: message lacks '$2': $err" ;; esac
    }
    expect_fail_early "a script without a harness" "no stub-agent harness"
    cat >"$wf/a.test.mjs" <<'EOF'
import { readFileSync } from 'node:fs'
const src = readFileSync(process.argv[2], 'utf8')
if (!src.includes("phase('Two')")) { console.error('harness: phase Two missing'); process.exit(1) }
EOF
    check_dir "$wf" "$rules" >/dev/null 2>&1 || self_test_fail "a good script with top-level await and return failed"
    good | sed "s/phase('Two')/phase('One')/" >"$wf/a.js"
    expect_fail_early "a failing harness" "harness: phase Two missing"

    expect_fail() {
        local label=$1 needle=$2
        local err
        if err=$(check_dir "$wf" "$rules" 2>&1); then
            self_test_fail "$label passed"
        fi
        case "$err" in *"$needle"*) ;; *) self_test_fail "$label: message lacks '$needle': $err" ;; esac
    }
    { echo "const x = 1"; good; } >"$wf/a.js"; expect_fail "meta not first" "first statement"
    good | sed '/description:/d' >"$wf/a.js"; expect_fail "meta without description" "no \`description\`"
    { good; echo "const broken = ("; } >"$wf/a.js"; expect_fail "a syntax error" "node --check failed"
    good | sed "s/phase('Two')/phase('Three')/" >"$wf/a.js"; expect_fail "an unknown phase" "phase('Three')"
    local seed
    for seed in "/var/home/someone/repo" "/home/someone" "/tmp/claude-1000/x" ".ssh/id_signing" \
        "ssh -i key" "GIT_SSH_COMMAND=x" "gh auth status" "Co-Authored-By: X" "someone@example.org"; do
        { good; printf 'log(%s)\n' "\`$seed\`"; } >"$wf/a.js"
        expect_fail "seed $seed" "$wf/a.js:12:"
    done
    # shellcheck disable=SC2016 # literal JavaScript template strings
    { good; echo 'log(`waits in the foreground and repeats a bounded call until the condition holds, never ending`)'; } >"$wf/a.js"
    expect_fail "a copied 12-word run" "sample.md"
    # shellcheck disable=SC2016 # literal JavaScript template strings
    { good; echo 'log(`A subagent or workflow stage waits in the foreground and repeats a bounded call until the CONDITION is true, never ending its turn`)'; } >"$wf/a.js"
    expect_fail "a copy with one word changed" "sample.md"
    good | sed 's/sample.md "Waiting"/sample.md "Sleeping"/' >"$wf/a.js"; expect_fail "a missing section" "not a heading"
    # Every title of a citation list is checked, not only the first (Fable S11),
    # across a parenthetical and an "and"; a quoted string after the list ends
    # is not a citation.
    good | sed 's/sample.md "Waiting"/sample.md "Waiting", "Sleeping"/' >"$wf/a.js"
    expect_fail "a missing second section" '"Sleeping"'
    good | sed 's/sample.md "Waiting"/sample.md "Waiting" (wait here), and "Napping"/' >"$wf/a.js"
    expect_fail "a missing section after a parenthetical" '"Napping"'
    good | sed 's/sample.md "Waiting"/sample.md "Sample Rule", "Waiting"/' >"$wf/a.js"
    check_dir "$wf" "$rules" >/dev/null 2>&1 || self_test_fail "a list of existing sections failed"
    good | sed 's/sample.md "Waiting" and stop./sample.md "Waiting". Return "ready, not a section"./' >"$wf/a.js"
    check_dir "$wf" "$rules" >/dev/null 2>&1 || self_test_fail "a quoted string after the citation list was read as a section"
    good >"$wf/a.js"
    local rc=0
    LUSHTEXT_NODE=/nonexistent/node check_dir "$wf" "$rules" >/dev/null 2>"$tmp/err" || rc=$?
    if [ "$rc" -eq 0 ] || ! grep -q "node not found" "$tmp/err"; then
        self_test_fail "a missing node did not fail with a message"
    fi
    echo "check-workflow-scripts self-test: ok (empty dir passes; meta, wrapper with top-level await and return, node --check, phases, personal data and credentials, 12-word rule runs, section citations, missing node, missing and failing harnesses)"
}

case "${1:-}" in
    --self-test) self_test ;;
    *)
        workflows="$REPO_ROOT/.claude/workflows" rules="$REPO_ROOT/.agents/rules"
        while [ $# -gt 0 ]; do
            case "$1" in
                --workflows) workflows=${2:?}; shift 2 ;;
                --rules) rules=${2:?}; shift 2 ;;
                *) echo "usage: check-workflow-scripts.sh [--workflows DIR] [--rules DIR] | --self-test" >&2; exit 2 ;;
            esac
        done
        check_dir "$workflows" "$rules"
        ;;
esac
