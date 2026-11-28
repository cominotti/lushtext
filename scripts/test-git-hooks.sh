#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Isolated scratch-repo self-tests for .githooks/pre-commit and .githooks/pre-push
# (.agents/rules/testing.md, "Test Isolation From Real App Data").

set -euo pipefail
export LC_ALL=C

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)

# Scrub git hook env vars so running inside a hook never hijacks fixture repos.
while IFS= read -r name; do
    [ -n "$name" ] && unset "$name"
done < <(git rev-parse --local-env-vars 2>/dev/null || true)

TMP_ROOT=$(mktemp -d)
trap 'rm -rf "$TMP_ROOT"' EXIT INT TERM

export GIT_CONFIG_GLOBAL=/dev/null
export GIT_CONFIG_NOSYSTEM=1

fail() {
    echo "test-git-hooks FAILED: $*" >&2
    exit 1
}

init_fixture() {
    local name=$1
    local repo="$TMP_ROOT/$name"
    local remote="$TMP_ROOT/$name-remote.git"
    git init -q --bare "$remote"
    git init -q -b main "$repo"
    git -C "$repo" config user.name "Test"
    git -C "$repo" config user.email "test@example.invalid"
    git -C "$repo" config commit.gpgsign false
    git -C "$repo" config core.hooksPath "$REPO_ROOT/.githooks"

    mkdir -p "$repo/scripts" "$repo/docs" "$repo/crates/lushtext-ui/src/ui"
    cp "$REPO_ROOT/scripts/precommit-scope.py" "$repo/scripts/precommit-scope.py"
    chmod +x "$repo/scripts/precommit-scope.py"

    cat >"$repo/Makefile" <<'EOF'
LOG ?= .git/make-calls.log
.PHONY: pre-commit pre-commit-docs check-smoke-proof
pre-commit:
	@echo "pre-commit" >> "$(LOG)"
pre-commit-docs:
	@echo "pre-commit-docs" >> "$(LOG)"
check-smoke-proof:
	@echo "check-smoke-proof $(SMOKE_PROOF_BASE_REF)" >> "$(LOG)"
	@if [ -n "$(SMOKE_PROOF_BASE_REF)" ] && ! git rev-parse --verify --quiet "$(SMOKE_PROOF_BASE_REF)^{commit}" >/dev/null 2>&1; then echo "bad base ref" >&2; exit 1; fi
	@if [ -f .git/fail-smoke-proof ]; then echo "smoke proof missing" >&2; exit 1; fi
EOF

    git -C "$repo" add -A
    git -C "$repo" -c core.hooksPath=/dev/null commit -q -m "initial"
    git -C "$repo" remote add origin "$remote"
    git -C "$repo" -c core.hooksPath=/dev/null push -q -u origin main
    git -C "$repo" checkout -q -b feature/test
    printf '%s\n' "$repo"
}

REPO=$(init_fixture "hooks")
LOG="$REPO/.git/make-calls.log"

# 1. Docs-only commit uses pre-commit-docs and skips full pre-commit.
: >"$LOG"
echo "updated docs" >>"$REPO/docs/guide.md"
git -C "$REPO" add docs/guide.md
git -C "$REPO" commit -q -m "docs: update guide"
grep -Fxq "pre-commit-docs" "$LOG" || fail "docs-only commit did not run pre-commit-docs"
if grep -Fxq "pre-commit" "$LOG"; then
    fail "docs-only commit unexpectedly ran full pre-commit"
fi

# 2. Unstamped Rust commit runs full pre-commit.
: >"$LOG"
echo "pub fn x() {}" >"$REPO/crates/lushtext-ui/src/ui/x.rs"
git -C "$REPO" add crates/lushtext-ui/src/ui/x.rs
git -C "$REPO" commit -q -m "feat: add x"
grep -Fxq "pre-commit" "$LOG" || fail "unstamped code commit did not run pre-commit"

# 3. Clean stamped Rust commit skips pre-commit via stamp-valid.
: >"$LOG"
echo "pub fn y() {}" >"$REPO/crates/lushtext-ui/src/ui/y.rs"
git -C "$REPO" add crates/lushtext-ui/src/ui/y.rs
(cd "$REPO" && python3 -B ./scripts/precommit-scope.py stamp-write >/dev/null 2>&1)
git -C "$REPO" commit -q -m "feat: add y"
if grep -Fxq "pre-commit" "$LOG" || grep -Fxq "pre-commit-docs" "$LOG"; then
    fail "clean stamped commit unexpectedly ran make pre-commit"
fi

# 4. Dirty worktree invalidates stamp and forces full pre-commit.
: >"$LOG"
echo "pub fn z() {}" >"$REPO/crates/lushtext-ui/src/ui/z.rs"
git -C "$REPO" add crates/lushtext-ui/src/ui/z.rs
(cd "$REPO" && python3 -B ./scripts/precommit-scope.py stamp-write >/dev/null 2>&1)
echo "// unstaged" >>"$REPO/crates/lushtext-ui/src/ui/z.rs"
git -C "$REPO" commit -q -m "feat: add z"
grep -Fxq "pre-commit" "$LOG" || fail "dirty worktree bypassed full pre-commit"
git -C "$REPO" checkout -- crates/lushtext-ui/src/ui/z.rs

# 5. pre-push blocks push when check-smoke-proof fails.
: >"$LOG"
touch "$REPO/.git/fail-smoke-proof"
if git -C "$REPO" push -q -u origin feature/test 2>/dev/null; then
    fail "pre-push allowed push while check-smoke-proof failed"
fi
grep -Fq "check-smoke-proof origin/main" "$LOG" || fail "pre-push did not invoke check-smoke-proof with origin/main"

# 6. pre-push succeeds once check-smoke-proof passes.
rm -f "$REPO/.git/fail-smoke-proof"
: >"$LOG"
git -C "$REPO" push -q -u origin feature/test
grep -Fq "check-smoke-proof origin/main" "$LOG" || fail "pre-push skipped check-smoke-proof on pass path"

# 6b. pre-push fails closed when SMOKE_PROOF_BASE_REF does not resolve.
git -C "$REPO" -c core.hooksPath=/dev/null commit -q --allow-empty -m "chore: trigger push"
if SMOKE_PROOF_BASE_REF="refs/heads/does-not-exist" git -C "$REPO" push -q origin feature/test 2>/dev/null; then
    fail "pre-push failed open on an unresolvable SMOKE_PROOF_BASE_REF"
fi

# 7. pre-push skips check-smoke-proof on ref deletion and tag pushes.
touch "$REPO/.git/fail-smoke-proof"
: >"$LOG"
git -C "$REPO" tag -a v0.0.1-test -m "test tag"
git -C "$REPO" push -q origin v0.0.1-test
if grep -Fq "check-smoke-proof" "$LOG"; then
    fail "pre-push ran check-smoke-proof on a tag push"
fi
git -C "$REPO" push -q origin --delete feature/test
if grep -Fq "check-smoke-proof" "$LOG"; then
    fail "pre-push ran check-smoke-proof on a branch deletion"
fi
rm -f "$REPO/.git/fail-smoke-proof"

# 8. Decoy GIT_DIR/GIT_INDEX_FILE/GIT_WORK_TREE must not break precommit-scope commands.
DECOY="$TMP_ROOT/decoy"
git init -q "$DECOY"
if ! GIT_DIR="$DECOY/.git" GIT_WORK_TREE="$DECOY" GIT_INDEX_FILE="$DECOY/.git/index" \
    python3 -B "$REPO_ROOT/scripts/precommit-scope.py" --self-test >/dev/null 2>&1; then
    fail "precommit-scope --self-test failed under decoy GIT_DIR"
fi
if GIT_DIR="$DECOY/.git" GIT_WORK_TREE="$DECOY" \
    python3 -B "$REPO_ROOT/scripts/precommit-scope.py" docs-only-range HEAD HEAD >/dev/null 2>&1; then
    fail "docs-only-range accepted empty/invalid range under decoy GIT_DIR"
fi
(cd "$REPO" && GIT_DIR="$DECOY/.git" GIT_WORK_TREE="$DECOY" \
    python3 -B "$REPO_ROOT/scripts/precommit-scope.py" stamp-write >/dev/null 2>&1) \
    || fail "precommit-scope stamp-write failed under decoy GIT_DIR"
if ! GIT_DIR="$DECOY/.git" GIT_WORK_TREE="$DECOY" GIT_INDEX_FILE="$DECOY/.git/index" \
    python3 -B "$REPO_ROOT/scripts/check-accessibility-policy.py" --self-test --static-only >/dev/null 2>&1; then
    fail "check-accessibility-policy --self-test failed under decoy GIT_DIR"
fi
if ! GIT_DIR="$DECOY/.git" GIT_WORK_TREE="$DECOY" GIT_INDEX_FILE="$DECOY/.git/index" \
    python3 -B "$REPO_ROOT/scripts/check-visual-proof-policy.py" --self-test >/dev/null 2>&1; then
    fail "check-visual-proof-policy --self-test failed under decoy GIT_DIR"
fi
if grep -Eq "bare = true|name = t" "$DECOY/.git/config"; then
    fail "self-tests leaked git config into decoy GIT_DIR"
fi

echo "test-git-hooks: ok (docs-only, clean stamp hit, dirty stamp miss, pre-push block/pass, tag/delete skip, decoy GIT_DIR ignored)"
