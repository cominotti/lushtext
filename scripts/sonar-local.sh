#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

# Fetch SonarQube Cloud quality gate status and unresolved issues via REST API.
# Fails when SonarQube Cloud reports any unresolved issue: each finding must be
# fixed in code or explicitly accepted/marked false-positive in SonarQube Cloud.
#
# Uses uploaded CI-based analysis results. This script does not run the scanner.
#
# Requires: curl, jq
# Environment:
#   SONAR_TOKEN       (optional: enables auth for private projects / higher limits)
#   SONAR_HOST_URL    (default: https://sonarcloud.io)
#   SONAR_PROJECT_KEY (default: cominotti_lushtext)
#   SONAR_BRANCH      (default: current git branch)
#   SONAR_PAGE_SIZE   (default: 500)
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

SONAR_HOST_URL="${SONAR_HOST_URL:-https://sonarcloud.io}"
SONAR_PROJECT_KEY="${SONAR_PROJECT_KEY:-cominotti_lushtext}"
SONAR_BRANCH="${SONAR_BRANCH:-}"
SONAR_PAGE_SIZE="${SONAR_PAGE_SIZE:-500}"

REPORT_DIR=".sonar/reports"
QUALITY_GATE_JSON="$REPORT_DIR/quality-gate.json"
ISSUES_JSON="$REPORT_DIR/issues.json"
BRANCHES_JSON="$REPORT_DIR/branches.json"
ANALYSES_JSON="$REPORT_DIR/analyses.json"

fail() {
	echo "ERROR: $*" >&2
	exit 1
}

info() {
	echo "==> $*"
}

require_cmd() {
	local cmd="$1"
	command -v "$cmd" >/dev/null 2>&1 || fail "Required command not found: $cmd"
}

get_current_branch() {
	local branch
	branch="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || true)"
	if [[ "$branch" == "HEAD" ]]; then
		echo ""
		return
	fi
	echo "$branch"
}

# Wraps curl with optional Bearer auth.
# SonarQube Cloud accepts unauthenticated requests for public projects.
# When SONAR_TOKEN is set, use it for private access and higher rate limits.
sonar_curl() {
	local -a args=(-fsS)
	if [[ -n "${SONAR_TOKEN:-}" ]]; then
		args+=(-H "Authorization: Bearer ${SONAR_TOKEN}")
	fi
	curl "${args[@]}" "$@"
}

write_no_data_reports() {
	local branch="$1"
	local note="$2"

	jq -n --arg branch "$branch" --arg note "$note" '{
		projectStatus: {
			status: "NONE",
			branch: $branch,
			note: $note
		}
	}' >"$QUALITY_GATE_JSON"
	printf '[]\n' >"$ISSUES_JSON"
}

fetch_project_analyses() {
	sonar_curl \
		--get "${SONAR_HOST_URL%/}/api/project_analyses/search" \
		--data-urlencode "project=${SONAR_PROJECT_KEY}" \
		--data-urlencode "ps=1" >"$ANALYSES_JSON"
}

project_has_any_analysis() {
	jq -e '(.paging.total // 0) > 0' "$ANALYSES_JSON" >/dev/null
}

fetch_branches() {
	sonar_curl \
		--get "${SONAR_HOST_URL%/}/api/project_branches/list" \
		--data-urlencode "project=${SONAR_PROJECT_KEY}" >"$BRANCHES_JSON"
}

sonar_branch_exists() {
	local branch="$1"

	if [[ -z "$branch" ]]; then
		return 0
	fi

	jq -e --arg branch "$branch" 'any(.branches[]?; .name == $branch)' "$BRANCHES_JSON" >/dev/null
}

check_quality_gate() {
	local branch="$1"
	local response gate_status failing_summary
	local -a curl_args

	curl_args=(
		--get "${SONAR_HOST_URL%/}/api/qualitygates/project_status"
		--data-urlencode "projectKey=${SONAR_PROJECT_KEY}"
	)
	if [[ -n "$branch" ]]; then
		curl_args+=(--data-urlencode "branch=${branch}")
	fi

	response="$(sonar_curl "${curl_args[@]}")"
	printf '%s\n' "$response" >"$QUALITY_GATE_JSON"
	gate_status="$(jq -r '.projectStatus.status // ""' <<<"$response")"

	case "$gate_status" in
	OK)
		echo "Sonar quality gate: OK"
		;;
	NONE | "")
		echo "Sonar quality gate: no data available"
		return 2
		;;
	ERROR)
		failing_summary="$(
			jq -r '
				.projectStatus.conditions[]
				| select(.status == "ERROR")
				| "\(.metricKey)=\(.actualValue // "-") (threshold \(.errorThreshold // "-"))"
			' <<<"$response"
		)"
		if [[ -n "$failing_summary" ]]; then
			fail "Sonar quality gate failed: ${failing_summary//$'\n'/; }"
		fi
		fail "Sonar quality gate failed with status: ERROR"
		;;
	*)
		fail "Unexpected quality gate status: ${gate_status}"
		;;
	esac
}

fetch_unresolved_issues() {
	local branch="$1"
	local page=1
	local total=0
	local page_count=0
	local retrieved=0
	local response
	local all_issues="[]"
	local -a curl_args

	while :; do
		curl_args=(
			--get "${SONAR_HOST_URL%/}/api/issues/search"
			--data-urlencode "componentKeys=${SONAR_PROJECT_KEY}"
			--data-urlencode "resolved=false"
			--data-urlencode "p=${page}"
			--data-urlencode "ps=${SONAR_PAGE_SIZE}"
		)
		if [[ -n "$branch" ]]; then
			curl_args+=(--data-urlencode "branch=${branch}")
		fi

		response="$(sonar_curl "${curl_args[@]}")"
		if (( page == 1 )); then
			total="$(jq -r '.paging.total // .total // 0' <<<"$response")"
		fi

		all_issues="$(jq -s '.[0] + [.[1].issues[]?]' <<<"${all_issues}"$'\n'"${response}")"
		page_count="$(jq -r '.issues | length' <<<"$response")"
		retrieved=$((retrieved + page_count))

		if (( page_count == 0 || retrieved >= total )); then
			break
		fi

		page=$((page + 1))
	done

	printf '%s\n' "$all_issues" >"$ISSUES_JSON"
}

print_issue_table() {
	local issue_count="$1"
	local table_rows

	if (( issue_count == 0 )); then
		echo "No unresolved issues found."
		return 0
	fi

	printf '%s\n' ""
	printf '%s\n' "SEVERITY	TYPE	RULE	FILE:LINE	MESSAGE"
	table_rows="$(
		jq -r '
			.[] |
			[
				(.severity // "-"),
				(.type // "-"),
				(.rule // "-"),
				(
					(
						(.component // "-")
						| split(":")
						| if length > 1 then .[1] else .[0] end
					)
					+ ":" +
					((.line // "-") | tostring)
				),
				((.message // "-") | gsub("[\r\n\t]+"; " "))
			]
			| @tsv
		' "$ISSUES_JSON"
	)"

	if command -v column >/dev/null 2>&1; then
		printf '%s\n' "$table_rows" | column -t -s $'\t'
	else
		printf '%s\n' "$table_rows"
	fi
}

main() {
	require_cmd curl
	require_cmd jq

	if ! [[ "$SONAR_PAGE_SIZE" =~ ^[1-9][0-9]*$ ]]; then
		fail "SONAR_PAGE_SIZE must be a positive integer"
	fi

	mkdir -p "$REPORT_DIR"
	rm -f "$QUALITY_GATE_JSON" "$ISSUES_JSON" "$BRANCHES_JSON" "$ANALYSES_JSON"

	local branch="$SONAR_BRANCH"
	if [[ -z "$branch" ]]; then
		branch="$(get_current_branch)"
	fi

	local dashboard_url="${SONAR_HOST_URL%/}/dashboard?id=${SONAR_PROJECT_KEY}"
	if [[ -n "$branch" ]]; then
		dashboard_url+="&branch=${branch}"
	fi
	echo "Sonar dashboard: $dashboard_url"

	info "Checking Sonar project analysis history"
	fetch_project_analyses

	info "Checking Sonar branch list"
	fetch_branches
	if [[ -n "$branch" ]] && ! sonar_branch_exists "$branch"; then
		if [[ "$branch" == "main" ]]; then
			fail "Sonar branch ${branch} was not found for project ${SONAR_PROJECT_KEY}"
		fi
		write_no_data_reports "$branch" "No SonarQube Cloud analysis exists for this branch yet"
		echo "Sonar quality gate: no data available for branch ${branch} (analysis may be pending or require a trusted PR)"
		echo "Unresolved Sonar issues: 0 (no branch analysis available)"
		echo "Quality gate JSON report: $QUALITY_GATE_JSON"
		echo "Issues JSON report: $ISSUES_JSON"
		return 0
	fi

	if [[ "${branch:-main}" == "main" ]] && ! project_has_any_analysis; then
		write_no_data_reports "${branch:-main}" "No SonarQube Cloud analysis exists for the main branch yet"
		fail "SonarQube Cloud project ${SONAR_PROJECT_KEY} has no recorded analysis for main"
	fi

	info "Checking Sonar quality gate"
	set +e
	check_quality_gate "$branch"
	local gate_status=$?
	set -e
	local gate_unavailable=0
	if (( gate_status == 2 )); then
		gate_unavailable=1
		write_no_data_reports "$branch" "No SonarQube Cloud quality gate status is available for this branch"
		echo "Sonar quality gate: no data available for branch ${branch:-main}; continuing with unresolved issue verification"
	elif (( gate_status != 0 )); then
		return "$gate_status"
	fi

	info "Fetching unresolved Sonar issues"
	fetch_unresolved_issues "$branch"

	local issue_count
	issue_count="$(jq -r 'length' "$ISSUES_JSON")"
	echo "Unresolved Sonar issues: $issue_count"
	echo "Quality gate JSON report: $QUALITY_GATE_JSON"
	echo "Issues JSON report: $ISSUES_JSON"
	print_issue_table "$issue_count"

	if (( issue_count > 0 )); then
		fail "Sonar has ${issue_count} unresolved issue(s); fix them or mark them accepted/false-positive in SonarQube Cloud"
	fi

	if (( gate_unavailable != 0 )); then
		echo "Sonar quality gate status was unavailable, but uploaded analysis exists and no unresolved issues were reported."
	fi
}

main "$@"
