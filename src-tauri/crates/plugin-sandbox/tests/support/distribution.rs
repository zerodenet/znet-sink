use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::SigningKey;
use znet_plugin_sandbox::{
    contract::*,
    distribution::{
        directory::Registration,
        package::{self, Payload, SourceComponent},
    },
};
pub const SEED: [u8; 32] = [17; 32];
pub fn registration() -> Registration {
    serde_json::from_value(serde_json::json!({
        "id":"org.example.plugin", "repository":"https://github.com/example/plugin", "publisher":{"id":"example","public_key":STANDARD.encode(SigningKey::from_bytes(&SEED).verifying_key().to_bytes())},
        "name":"Example", "description":"Test", "license":"MIT", "maintainers":["example"],
        "release_source":{"type":"github-releases","metadata_asset":"marketplace-entry.json"}, "surfaces":[],"capabilities":["plugin.self.read"]
    })).unwrap()
}
pub fn payload(version: &str, targets: serde_json::Value) -> Payload {
    let source = "JSON.parse(hostCall('{\"capability\":\"plugin.self.read\",\"scope\":\"self\"}'))";
    let manifest: Manifest = serde_json::from_value(serde_json::json!({
        "schema_version":1,"host":"znet-sink","plugin_id":"org.example.plugin","component_id":"identity","version":version,"requires_host":"=0.0.1", "api_version":1,"runtime":"javascript-v1","minimum_isolation":"vm","targets":targets,
        "required":[{"capability":"plugin.self.read","scope":"self"}],"optional":[],"source_sha256":sha256(source.as_bytes()),"limits":Limits::default()
    })).unwrap();
    Payload {
        schema_version: 1,
        host: "znet-sink".into(),
        plugin_id: "org.example.plugin".into(),
        version: version.into(),
        components: vec![SourceComponent {
            manifest,
            source: source.into(),
        }],
    }
}
pub fn signed(payload: &Payload) -> Vec<u8> {
    package::sign(&serde_json::to_vec(payload).unwrap(), &SEED).unwrap()
}
