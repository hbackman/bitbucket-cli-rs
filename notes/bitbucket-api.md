# Bitbucket Cloud REST API — Research Notes

> Atlassian's docs pages are JS-heavy and the WebFetch read returned truncated content. The shape below is from prior knowledge + the developer.atlassian.com URL structure. **Verify each endpoint before implementation.**

## Basics

- Base URL: `https://api.bitbucket.org/2.0/`
- Auth: `Authorization: Bearer <oauth_token>` for OAuth flows; Basic auth with `username:api_token` for API-token use.
- Pagination: cursor-style with `next`/`previous` URLs in the envelope. Body shape:
  ```json
  { "values": [...], "pagelen": 10, "page": 1, "next": "https://api.bitbucket.org/..." }
  ```
- Rate limits: per-user / per-IP / per-app. Returns `429` with `Retry-After` when exceeded.
- All write endpoints return the full updated resource (no need for a second GET).

## Endpoints we care about for MVP

### Repositories
- `GET /repositories/{workspace}` — list repos in a workspace
- `GET /repositories/{workspace}/{repo_slug}` — view a repo
- `POST /repositories/{workspace}/{repo_slug}` — create a repo (workspace + slug in path; settings in body)
- `DELETE /repositories/{workspace}/{repo_slug}`
- `POST /repositories/{workspace}/{repo_slug}/forks` — fork
- `GET /repositories/{workspace}/{repo_slug}/branches`
- `GET /repositories/{workspace}/{repo_slug}/refs/branches/{name}`

### Pull requests
- `GET /repositories/{workspace}/{repo_slug}/pullrequests` — list (filter by `state=OPEN|MERGED|DECLINED|SUPERSEDED`)
- `POST /repositories/{workspace}/{repo_slug}/pullrequests` — create
- `GET /repositories/{workspace}/{repo_slug}/pullrequests/{pr_id}` — view
- `PUT /repositories/{workspace}/{repo_slug}/pullrequests/{pr_id}` — edit
- `POST .../pullrequests/{pr_id}/merge` — merge (body: merge strategy, message, close source branch)
- `POST .../pullrequests/{pr_id}/decline` — close (Bitbucket calls this "decline")
- `POST .../pullrequests/{pr_id}/approve` / `DELETE` — approve / unapprove
- `POST .../pullrequests/{pr_id}/request-changes` / `DELETE`
- `GET .../pullrequests/{pr_id}/diff`
- `GET .../pullrequests/{pr_id}/diffstat`
- `GET .../pullrequests/{pr_id}/commits`
- `GET .../pullrequests/{pr_id}/comments` and `POST` to add
- `GET .../pullrequests/{pr_id}/statuses` — build statuses (CI checks)
- `GET .../pullrequests/{pr_id}/activity` — combined timeline

Note: there's a separate **tasks** concept on PRs (`/tasks`), Bitbucket-specific, no clean gh analog. Skip for MVP.

### Issues
- `GET/POST /repositories/{workspace}/{repo_slug}/issues`
- `GET/PUT /repositories/{workspace}/{repo_slug}/issues/{issue_id}`
- `POST .../issues/{issue_id}/comments`

### Pipelines (post-MVP)
- `GET /repositories/{workspace}/{repo_slug}/pipelines/`
- `POST /repositories/{workspace}/{repo_slug}/pipelines/` — trigger a run
- `GET .../pipelines/{uuid}` — view
- `GET .../pipelines/{uuid}/steps` and `/steps/{step_uuid}/log`

### User / workspace
- `GET /user` — current user info (for `bb auth status`)
- `GET /workspaces` — list workspaces accessible by the user

## Naming and concept differences vs GitHub

| GitHub | Bitbucket | Note |
| --- | --- | --- |
| `owner/repo` | `workspace/repo_slug` | Different identifier; resolved differently from `git remote`. |
| Close a PR | "Decline" a PR | We can keep `bb pr close` as a friendly alias. |
| Reviewers requested | Reviewers (same concept) | Approval is per-reviewer; Bitbucket has no "review" object that wraps comments. |
| PR Review (approve/changes/comment) | Approve / Request Changes / inline comments | No single combined `bb pr review` body; we'd compose from primitives. |
| Checks / Statuses | "Build statuses" linked to commits/PRs | `bb pr checks` should hit `/statuses`. |
| Default branch | Mainbranch | Different field name in API responses. |
| Issues + Discussions | Issues only | Bitbucket has no Discussions. |
| GitHub Actions | Bitbucket Pipelines | Different config (`bitbucket-pipelines.yml`) and API shape. |
| GraphQL | REST only (no GraphQL) | We don't get gh's GraphQL escape hatch. Means more REST calls per `bb pr view`. |

## Resolving "current repo" from a git remote

We need a function that:
1. Reads `git config --get remote.origin.url` (and `upstream`).
2. Parses common Bitbucket URL forms:
   - `git@bitbucket.org:workspace/repo.git` (SSH)
   - `https://bitbucket.org/workspace/repo.git` (HTTPS)
   - `ssh://git@bitbucket.org/workspace/repo` (alternative SSH)
3. Returns `{workspace, repo_slug}`.

Like `gh`, support `--repo workspace/repo` to override.

## API client design

Should expose three layers:

1. **Low-level transport** — `Client.do(method, path, body) -> Response`. Handles auth header, retry on 429, token refresh on 401, JSON encode/decode.
2. **Resource methods** — `client.PullRequests().List(workspace, repo, opts)` etc. Typed structs.
3. **`bb api`** escape hatch — pass-through to (1) for any URL the user supplies.

Pagination should be exposed as an iterator/stream (Go: channel or callback; Rust: `Stream` or paginating iterator).
