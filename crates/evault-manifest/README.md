# evault-manifest

[![crates.io](https://img.shields.io/crates/v/evault-manifest.svg)](https://crates.io/crates/evault-manifest)
[![docs.rs](https://docs.rs/evault-manifest/badge.svg)](https://docs.rs/evault-manifest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/stescobedo92/hide-env-keys/blob/master/LICENSE)

> Parser and serializer for the `evault.toml` project manifest. Implements [`evault-core`](https://crates.io/crates/evault-core)'s `ManifestIo` trait by reading and writing TOML files on disk.

## On-disk format

```toml
project_id = "550e8400-e29b-41d4-a716-446655440000"
name = "my-app"

[vars]
NODE_ENV = "production"                     # inline literal
DATABASE_URL = { ref = "uuid-of-var" }      # reference to the registry

[profiles.dev]
NODE_ENV = "development"
DATABASE_URL = { ref = "uuid-of-dev-db" }
```

Rules:

- `[vars]` lists the **default-profile** bindings.
- `[profiles.<name>]` lists overrides for a named profile.
- Each binding is either a literal string or a `{ ref = "<uuid>" }` reference.

## Atomic writes

`FileManifestIo::save` writes through a sibling tempfile and `fs::rename`s into place, so a process crash can never leave a half-written manifest at the target path.

## Install

```toml
[dependencies]
evault-core = "0.1"
evault-manifest = "0.1"
```

## Example: round-trip a manifest

```rust
use evault_core::model::{
    BindingSource, ManifestBinding, ManifestSnapshot, Profile, ProjectId,
};
use evault_core::traits::ManifestIo;
use evault_manifest::FileManifestIo;

let dir = tempfile::tempdir().unwrap();
let path = dir.path().join("evault.toml");

let snapshot = ManifestSnapshot::new(
    ProjectId::new_v4(),
    "my-app".to_owned(),
    vec![ManifestBinding {
        key: "NODE_ENV".to_owned(),
        profile: Profile::default_profile(),
        source: BindingSource::Inline { value: "production".to_owned() },
    }],
);

let io = FileManifestIo;
io.save(&path, &snapshot).unwrap();

let loaded = io.load(&path).unwrap();
assert_eq!(loaded.bindings.len(), 1);
assert_eq!(loaded.bindings[0].key, "NODE_ENV");
```

## Part of the evault workspace

Used by [evault](https://github.com/stescobedo92/hide-env-keys)'s `link` / `gen` / `run` flows to read and write per-project manifests. See the workspace README for the broader architecture.

## License

[MIT](https://github.com/stescobedo92/hide-env-keys/blob/master/LICENSE)
