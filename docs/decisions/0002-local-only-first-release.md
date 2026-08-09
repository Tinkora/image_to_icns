# ADR-0002: Keep the first release local-only

## Status

Accepted

## Date

2026-08-09

## Context

An early design included a `share_result` mode backed by Cloudflare R2 and an
MCP tool named `get_icon_artifact`. No artifact upload, download authorization,
retention enforcement, abuse control, or end-to-end user flow existed for that
mode. Advertising it would overstate the product and create storage and
security obligations before there is evidence that users need server-hosted
icons.

The verified workflow already solves the primary problem: a user creates and
downloads a complete `.icns` file without uploading artwork.

## Decision

Version 0.1 is local-only:

- The browser stores neither source images nor generated files.
- The optional Worker stores Session metadata in D1 only.
- MCP exposes create, query, and cancel Session tools.
- Completion metadata contains byte length and representation count, not an
  artifact URL.
- R2 bindings, `share_result`, and `get_icon_artifact` are excluded from the
  public contract.

## Alternatives considered

### Ship the unfinished artifact API as experimental

This would still create an observable public contract and invite users to rely
on behavior that is not implemented.

### Implement artifact storage before the first release

This would delay the useful local editor and add authorization, retention,
content-type validation, quotas, and abuse handling without validated demand.

## Consequences

- The privacy claim is simple and testable.
- The Worker remains inexpensive and optional.
- Agents can observe completion but cannot retrieve a user's local download.
- Artifact sharing can be reconsidered only with a concrete user workflow and
  a complete security and retention design.
