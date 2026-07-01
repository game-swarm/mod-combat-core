# Swarm Mod: combat-core

Core combat systems — melee, ranged, healing, damage application, and death for Swarm
int
string

## Directory Structure

```
mods/combat-core/
├── Cargo.toml        # Static Bevy Plugin crate
├── mod.toml          # Mod metadata + configurable parameters
├── src/lib.rs        # `impl Plugin` entry point
└── README.md
```

## Configuration

See `mod.toml` for all configurable parameters. Server operators can override via:

```bash
swarm mod config combat-core <key> <value>
```

Or in `world.toml`:

```toml
[mods.combat-core.config]
# key = value
```

## Engine API

Mods are statically compiled Bevy Plugin crates. Enable this mod with the
`mod_combat_core` Cargo feature, or with `vanilla_mods`.

## Publishing

```bash
git tag v0.1.0
git push --tags
swarm mod pack
```
