---
title: Project manifest format
description: Grammar and semantics of the evault.toml file that binds a project to the central registry.
---

Every project that opts into evault carries an `evault.toml` at its root. The manifest declares which variables the project consumes, where their values come from (the central registry or an inline literal), and under which profile each binding applies.

`evault.toml` is **declarative** — it describes the bindings, not how to materialise them. The `gen` and `run` subcommands read it and resolve values at the moment of use.

## Top-level shape

```toml
project_id = "550e8400-e29b-41d4-a716-446655440000"
name = "my-app"

[[bindings]]
key = "DATABASE_URL"
profile = "default"
source = { kind = "registry", var_id = "<DATABASE_URL-uuid>" }

[[bindings]]
key = "NODE_ENV"
profile = "default"
source = { kind = "inline", value = "production" }
```

| Field | Type | Required | Notes |
|---|---|---|---|
| `project_id` | UUID v4 string | Yes | Stable identifier for this project. Generated on first `evault link`. |
| `name` | string | Yes | Human-readable label. Shown in the TUI's project view. |
| `bindings` | array of tables | No (empty array is valid) | Every variable this project consumes. |

## A binding

Each entry in `[[bindings]]`:

| Field | Type | Required | Notes |
|---|---|---|---|
| `key` | string | Yes | The environment-variable name exposed to the process. Validated against `[A-Za-z_][A-Za-z0-9_]{0,63}`. |
| `profile` | string | No (default: `"default"`) | The profile under which this binding applies. |
| `source` | table | Yes | Where the value comes from. See below. |

The `source` table has one required field, `kind`, with two valid values:

### `source = { kind = "registry", var_id = "<uuid>" }`

The value comes from the central registry. Only the UUID is stored in `evault.toml`; the value never appears in the file.

This is the right pick for **anything you do not want committed to the project's git repo**. The manifest is safe to commit — it only references the value indirectly.

### `source = { kind = "inline", value = "..." }`

The value is embedded directly in the manifest as a TOML string. Use only for non-sensitive literals like `NODE_ENV=production` or `LOG_LEVEL=info`.

Inline values are forwarded verbatim to the child process. They are still validated for the same NUL / CRLF rules as registry-sourced values.

## Profiles

The same `key` can appear in multiple bindings, each under a different `profile`. The resolution rule:

1. The active profile's binding wins, if one exists.
2. Otherwise, the `default` profile's binding (if any) is used.
3. Otherwise, the key is not exposed to the child process.

This means a binding in `default` works as a fallback for every profile that does not override it. See [Profiles](/hide-env-keys/cli/profiles/) for worked examples.

## Atomic writes

When `evault link` or `evault gen` updates the manifest, the write goes through a sibling tempfile and `fs::rename`, on the same volume as the target. A process crash mid-write never leaves a half-written `evault.toml` visible at the target path.

## Schema evolution

The current shape is **v1**. There is no version field at the top of the manifest yet because v1 is the only shape that exists. If the format changes incompatibly, a future version will introduce a `manifest_version` field and the parser will reject anything it does not recognise.

A `project_id` is stable across renames and moves. Two manifests pointing at the same `project_id` are treated as the same project, even if their `name` differs — useful when you fork a project and want to share its registry references.

## Should I commit `evault.toml` to git?

**Yes.** The manifest is part of the project's source code: it documents which variables the project consumes and which profiles it supports. Anyone cloning the repo can run `evault gen` or `evault run` against their own registry without having to reconstruct the binding map manually.

The values themselves are not in the manifest (registry bindings only carry UUIDs; inline values are non-sensitive by convention), so the file is safe to publish.
