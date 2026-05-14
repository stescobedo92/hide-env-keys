---
title: Profiles
description: How profiles let one project pull different values per environment (dev / staging / prod) without duplicating variable names.
---

A **profile** is a named view of a project's bindings. The same variable can resolve to different values depending on which profile is active, without duplicating variable names in the registry.

## When you need profiles

The canonical example is a database URL: in `dev` you point at `localhost`, in `staging` at a remote test DB, in `prod` at the production cluster. The variable's **name** in the project's environment is the same (`DATABASE_URL`) but the **value** differs by environment.

Without profiles you would have to create three variables (`DATABASE_URL_DEV`, `DATABASE_URL_STAGING`, `DATABASE_URL_PROD`) and rewrite your application code to read the right one based on `NODE_ENV` or similar. Profiles invert this: your code reads `DATABASE_URL`, and evault picks which underlying variable resolves to it based on the profile in effect at materialise / run time.

## Wire up

Create one variable per environment:

```bash
evault add DATABASE_URL_DEV --secret
evault add DATABASE_URL_STAGING --secret
evault add DATABASE_URL_PROD --secret
```

Then link each to the project under its profile, with a shared alias:

```bash
evault link DATABASE_URL_DEV --project ./api \
    --profile dev --alias DATABASE_URL
evault link DATABASE_URL_STAGING --project ./api \
    --profile staging --alias DATABASE_URL
evault link DATABASE_URL_PROD --project ./api \
    --profile prod --alias DATABASE_URL
```

The resulting `evault.toml` looks like:

```toml
project_id = "..."
name = "api"

[[bindings]]
key = "DATABASE_URL"
profile = "dev"
source = { kind = "registry", var_id = "<DATABASE_URL_DEV-uuid>" }

[[bindings]]
key = "DATABASE_URL"
profile = "staging"
source = { kind = "registry", var_id = "<DATABASE_URL_STAGING-uuid>" }

[[bindings]]
key = "DATABASE_URL"
profile = "prod"
source = { kind = "registry", var_id = "<DATABASE_URL_PROD-uuid>" }
```

## Use a profile

Every subcommand that resolves bindings accepts `--profile`:

```bash
# Generate the dev .env
evault gen --project ./api --profile dev

# Spawn the server pointing at staging
evault run --project ./api --profile staging -- ./serve

# Materialise the prod env into a CI runner's environment
evault gen --project ./api --profile prod
```

The default profile is, unsurprisingly, `default`. Bindings without a `profile = "..."` entry sit in `default` and are always picked up unless an override exists for the active profile.

## Overrides

When the active profile has a binding for a key, it **replaces** the default-profile binding for that key. When it doesn't, the default-profile binding (if any) wins. So you can put `NODE_ENV = "production"` in the default profile and only override `DATABASE_URL` per environment, instead of repeating every key in every profile.

## Inline vs registry bindings

Bindings come in two flavours:

- **`registry`** — value lives in the central registry; only the variable's UUID is stored in `evault.toml`. The right pick for any value you don't want in your repo.
- **`inline`** — non-sensitive literal embedded directly in the manifest. Fine for things like `NODE_ENV=production`; never use for credentials.

Both kinds work the same way under profiles. See [Manifest format](/hide-env-keys/reference/manifest/) for the full grammar.
