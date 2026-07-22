# Bundled routing-rule data

The generated JSON and ZRS files in this directory are derived from
`MetaCubeX/meta-rules-dat` at commit
`f1fedafc389862084dab3ff0232e856bcfbbc042`.

- Upstream: https://github.com/MetaCubeX/meta-rules-dat
- License: GNU General Public License v3.0
- License revision: `4178770badecb1b349fbcd62c737e0d7a2079729` (`master`)
- Generator: `cargo run --manifest-path src-tauri/Cargo.toml --example build_builtin_rules`

The generated files are data resources. ZNet Sink keeps the upstream source
revision and per-resource SHA-256 values in `manifest.json` so the snapshot can
be reproduced and audited.
