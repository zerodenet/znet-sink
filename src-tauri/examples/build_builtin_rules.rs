use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zero_rule::protocol::encode_json;
use zero_rule::zrs::{encode, verify, VerifyMode};
use zero_rule::{Rule, RuleSet, RuleSetCompiler};

const SOURCE_REPOSITORY: &str = "https://github.com/MetaCubeX/meta-rules-dat";
const DEFAULT_SOURCE_COMMIT: &str = "f1fedafc389862084dab3ff0232e856bcfbbc042";
const SOURCE_LICENSE: &str = "GPL-3.0-only";
const SOURCE_LICENSE_URL: &str =
    "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/4178770badecb1b349fbcd62c737e0d7a2079729/LICENSE";
const DEFAULT_SNAPSHOT_BUILT_AT_UNIX_MS: u64 = 1_784_688_400_068;

#[derive(Clone, Copy)]
enum SourceKind {
    Domain,
    Ip,
}

struct Definition {
    id: &'static str,
    name: &'static str,
    relative_url: &'static str,
    kind: SourceKind,
    action: &'static str,
    order: u32,
}

const DEFINITIONS: &[Definition] = &[
    Definition {
        id: "builtin-private-ip",
        name: "私有网络地址",
        relative_url: "geo/geoip/private.yaml",
        kind: SourceKind::Ip,
        action: "direct",
        order: 10,
    },
    Definition {
        id: "builtin-cn-domain",
        name: "中国大陆域名",
        relative_url: "geo/geosite/cn.yaml",
        kind: SourceKind::Domain,
        action: "direct",
        order: 20,
    },
    Definition {
        id: "builtin-cn-ip",
        name: "中国大陆 IP",
        relative_url: "geo/geoip/cn.yaml",
        kind: SourceKind::Ip,
        action: "direct",
        order: 30,
    },
    Definition {
        id: "builtin-gfw-domain",
        name: "GFW 域名",
        relative_url: "geo/geosite/gfw.yaml",
        kind: SourceKind::Domain,
        action: "proxy",
        order: 40,
    },
];

#[derive(Deserialize)]
struct ClashPayload {
    payload: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema: &'static str,
    version: u32,
    generated_at_unix_ms: u64,
    source_repository: &'static str,
    source_commit: String,
    source_license: &'static str,
    assets: Vec<ManifestAsset>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestAsset {
    id: &'static str,
    name: &'static str,
    zrs_file: String,
    source_url: String,
    source_sha256: String,
    ir_sha256: String,
    zrs_checksum: u32,
    zrs_sha256: String,
    zrs_file_size: u64,
    entry_count: u64,
    default_action: &'static str,
    default_order: u32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source_commit = std::env::var("BUILTIN_RULE_SOURCE_COMMIT")
        .unwrap_or_else(|_| DEFAULT_SOURCE_COMMIT.to_string());
    if source_commit.len() != 40 || !source_commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("BUILTIN_RULE_SOURCE_COMMIT must be a 40-character Git commit SHA".into());
    }
    let generated_at_unix_ms = std::env::var("BUILTIN_RULE_GENERATED_AT_UNIX_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_SNAPSHOT_BUILT_AT_UNIX_MS);
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("builtin-rules");
    fs::create_dir_all(&output)?;
    let client = reqwest::blocking::Client::builder()
        .user_agent("znet-sink-builtin-rule-builder/1")
        .build()?;
    let license = client
        .get(SOURCE_LICENSE_URL)
        .send()?
        .error_for_status()?
        .bytes()?;
    fs::write(output.join("LICENSE.meta-rules-dat"), license)?;
    let mut assets = Vec::with_capacity(DEFINITIONS.len());

    for definition in DEFINITIONS {
        let source_url = format!(
            "https://raw.githubusercontent.com/MetaCubeX/meta-rules-dat/{source_commit}/{}",
            definition.relative_url
        );
        let source = client
            .get(&source_url)
            .send()?
            .error_for_status()?
            .bytes()?;
        let document: ClashPayload = serde_yaml::from_slice(&source)?;
        let rules = convert_rules(document.payload, definition.kind)?;
        let rule_set = RuleSet {
            display_name: Some(definition.name.to_string()),
            rules,
        };
        let ir = encode_json(&rule_set)?;
        let (compiled, report) = RuleSetCompiler.compile(rule_set)?;
        let zrs = encode(&compiled)?;
        let metadata = verify(&zrs, VerifyMode::FullChecksum)?;
        let zrs_file = format!("{}.zrs", definition.id);
        let legacy_ir_file = output.join(format!("{}.json", definition.id));
        if legacy_ir_file.exists() {
            fs::remove_file(legacy_ir_file)?;
        }
        fs::write(output.join(&zrs_file), &zrs)?;
        assets.push(ManifestAsset {
            id: definition.id,
            name: definition.name,
            zrs_file,
            source_url,
            source_sha256: sha256(&source),
            ir_sha256: sha256(&ir),
            zrs_checksum: metadata.body_checksum,
            zrs_sha256: sha256(&zrs),
            zrs_file_size: zrs.len() as u64,
            entry_count: report.output_entries as u64,
            default_action: definition.action,
            default_order: definition.order,
        });
    }

    let manifest = Manifest {
        schema: "znet.builtin-rules/v1",
        version: 2,
        generated_at_unix_ms,
        source_repository: SOURCE_REPOSITORY,
        source_commit,
        source_license: SOURCE_LICENSE,
        assets,
    };
    fs::write(
        output.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(())
}

fn convert_rules(
    payload: Vec<String>,
    kind: SourceKind,
) -> Result<Vec<Rule>, Box<dyn std::error::Error>> {
    payload
        .into_iter()
        .enumerate()
        .map(|(index, raw)| match kind {
            SourceKind::Domain => {
                let domain = raw
                    .strip_prefix("+.")
                    .or_else(|| raw.strip_prefix("*."))
                    .or_else(|| raw.strip_prefix('.'))
                    .ok_or_else(|| format!("unsupported domain entry at {index}: {raw}"))?;
                Ok(Rule::DomainSuffix(domain.to_string()))
            }
            SourceKind::Ip if raw.contains(':') => Ok(Rule::Ipv6Cidr(raw.parse()?)),
            SourceKind::Ip => Ok(Rule::Ipv4Cidr(raw.parse()?)),
        })
        .collect()
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
