//! sha256-json-v3 uses compact serde_json encoding (locked by Cargo.lock).
//! Dependency upgrades must preserve the fixed byte vectors or change ALGORITHM.
use anyhow::{Result, bail};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::shared::assets::Role;

pub const ALGORITHM: &str = "sha256-json-v3";

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn json(value: &toml::Value) -> Result<Value> {
    Ok(match value {
        toml::Value::String(v) => Value::String(v.clone()),
        toml::Value::Integer(v) => Value::from(*v),
        toml::Value::Float(v) => {
            if !v.is_finite() {
                bail!("清单包含非有限浮点数，无法生成 v3 指纹");
            }
            Value::from(*v)
        }
        toml::Value::Boolean(v) => Value::Bool(*v),
        toml::Value::Datetime(v) => Value::String(v.to_string().replace(' ', "T")),
        toml::Value::Array(v) => Value::Array(v.iter().map(json).collect::<Result<_>>()?),
        toml::Value::Table(v) => {
            let mut map = Map::new();
            for (key, value) in v {
                map.insert(key.clone(), json(value)?);
            }
            Value::Object(map)
        }
    })
}

// Explicitly sort recursively, independent of serde_json preserve_order features.
fn ordered(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<_> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            let mut map = Map::new();
            for (key, value) in entries {
                map.insert(key, ordered(value));
            }
            Value::Object(map)
        }
        Value::Array(values) => Value::Array(values.into_iter().map(ordered).collect()),
        value => value,
    }
}

pub fn encode(manifest: &toml::Value, roles: &[Role]) -> Result<Vec<u8>> {
    let mut roles: Vec<_> = roles.iter().collect();
    roles.sort_by(|a, b| a.name.cmp(&b.name));
    let value =
        serde_json::json!({"algorithm": ALGORITHM,"manifest":json(manifest)?,"roles":roles});
    Ok(serde_json::to_vec(&ordered(value))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_encoding_and_hash_vector() {
        let manifest: toml::Value = toml::from_str(
            "title = '中文<&>'\n[metadata]\nz = [1, 1.5, true]\nat = 1979-05-27 07:32:00Z\n",
        )
        .unwrap();
        let bytes = encode(&manifest, &[]).unwrap();
        let expected = "{\"algorithm\":\"sha256-json-v3\",\"manifest\":{\"metadata\":{\"at\":\"1979-05-27T07:32:00Z\",\"z\":[1,1.5,true]},\"title\":\"中文<&>\"},\"roles\":[]}";
        assert_eq!(bytes, expected.as_bytes());
        assert_eq!(
            digest(&bytes),
            "301dcba43b856ed75cdbe00cd081c37edf0a7840ea34006f722ac98ed7c847b5"
        );
        let reordered: toml::Value = toml::from_str(
            "title = '中文<&>'\n[metadata]\nat = 1979-05-27T07:32:00Z\nz = [1, 1.5, true]\n",
        )
        .unwrap();
        assert_eq!(bytes, encode(&reordered, &[]).unwrap());
    }
    #[test]
    fn role_nulls_are_part_of_the_fixed_vector() {
        let manifest: toml::Value = toml::from_str("name='fixture'\n").unwrap();
        let role = Role {
            name: "one".into(),
            file: "agents/one.toml".into(),
            modes: vec!["custom".into()],
            access: "task-scoped".into(),
            model: None,
            reasoning_effort: None,
            sha256: "file".into(),
            prompt_sha256: "prompt".into(),
        };
        let expected = r#"{"algorithm":"sha256-json-v3","manifest":{"name":"fixture"},"roles":[{"access":"task-scoped","file":"agents/one.toml","model":null,"modes":["custom"],"name":"one","prompt_sha256":"prompt","reasoning_effort":null,"sha256":"file"}]}"#;
        let bytes = encode(&manifest, &[role]).unwrap();
        assert_eq!(bytes, expected.as_bytes());
        assert_eq!(
            digest(&bytes),
            "bd3eb6bd74d90a52c68700e701bcc56aced8a7d5a49a10a1067a4cb4efe4ab35"
        );
    }
    #[test]
    fn date_and_local_time_encoding() {
        let manifest =
            toml::from_str("day=1979-05-27\ntime=07:32:00\noffset=1979-05-27T07:32:00+08:00\n")
                .unwrap();
        assert_eq!(
            String::from_utf8(encode(&manifest, &[]).unwrap()).unwrap(),
            r#"{"algorithm":"sha256-json-v3","manifest":{"day":"1979-05-27","offset":"1979-05-27T07:32:00+08:00","time":"07:32:00"},"roles":[]}"#
        );
    }
    #[test]
    fn nonfinite_is_rejected_at_every_depth() {
        for spelling in ["nan", "inf", "-inf"] {
            let value = toml::from_str(&format!("[metadata]\nvalues=[{spelling}]\n")).unwrap();
            assert!(
                encode(&value, &[])
                    .unwrap_err()
                    .to_string()
                    .contains("非有限")
            );
        }
    }
}
