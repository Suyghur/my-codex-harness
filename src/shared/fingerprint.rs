//! sha256-json-v3 uses compact serde_json encoding (locked by Cargo.lock).
//! Dependency upgrades must preserve the fixed byte vectors or change ALGORITHM.
use std::io::{self, Write};

use anyhow::Result;
use serde::{
    Serialize, Serializer,
    ser::{Error, SerializeMap, SerializeSeq, SerializeStruct},
};
use sha2::{Digest, Sha256};

use crate::shared::assets::Role;

pub const ALGORITHM: &str = "sha256-json-v3";

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

// Borrow parsed TOML instead of constructing and sorting a second value tree.
struct ManifestValue<'a>(&'a toml::Value);

impl Serialize for ManifestValue<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self.0 {
            toml::Value::String(value) => serializer.serialize_str(value),
            toml::Value::Integer(value) => serializer.serialize_i64(*value),
            toml::Value::Float(value) => {
                if !value.is_finite() {
                    return Err(S::Error::custom("清单包含非有限浮点数，无法生成 v3 指纹"));
                }
                serializer.serialize_f64(*value)
            }
            toml::Value::Boolean(value) => serializer.serialize_bool(*value),
            toml::Value::Datetime(value) => {
                serializer.serialize_str(&value.to_string().replace(' ', "T"))
            }
            toml::Value::Array(values) => {
                let mut sequence = serializer.serialize_seq(Some(values.len()))?;
                for value in values {
                    sequence.serialize_element(&ManifestValue(value))?;
                }
                sequence.end()
            }
            toml::Value::Table(values) => {
                let mut entries: Vec<_> = values.iter().collect();
                entries.sort_unstable_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
                let mut map = serializer.serialize_map(Some(entries.len()))?;
                for (key, value) in entries {
                    map.serialize_entry(key, &ManifestValue(value))?;
                }
                map.end()
            }
        }
    }
}

// The public Role's declaration order is an output choice, not the v3 key order.
struct FingerprintRole<'a>(&'a Role);

impl Serialize for FingerprintRole<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let role = self.0;
        let mut fields = serializer.serialize_struct("Role", 8)?;
        fields.serialize_field("access", &role.access)?;
        fields.serialize_field("file", &role.file)?;
        fields.serialize_field("model", &role.model)?;
        fields.serialize_field("modes", &role.modes)?;
        fields.serialize_field("name", &role.name)?;
        fields.serialize_field("prompt_sha256", &role.prompt_sha256)?;
        fields.serialize_field("reasoning_effort", &role.reasoning_effort)?;
        fields.serialize_field("sha256", &role.sha256)?;
        fields.end()
    }
}

#[derive(Serialize)]
struct Fingerprint<'a> {
    algorithm: &'static str,
    manifest: ManifestValue<'a>,
    roles: Vec<FingerprintRole<'a>>,
}

fn fingerprint<'a>(manifest: &'a toml::Value, roles: &'a [Role]) -> Fingerprint<'a> {
    let mut roles: Vec<_> = roles.iter().map(FingerprintRole).collect();
    roles.sort_by(|a, b| a.0.name.as_bytes().cmp(b.0.name.as_bytes()));
    Fingerprint {
        algorithm: ALGORITHM,
        manifest: ManifestValue(manifest),
        roles,
    }
}

struct HashWriter(Sha256);

impl Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub fn bundle_digest(manifest: &toml::Value, roles: &[Role]) -> Result<String> {
    let mut writer = HashWriter(Sha256::new());
    serde_json::to_writer(&mut writer, &fingerprint(manifest, roles))?;
    Ok(format!("{:x}", writer.0.finalize()))
}

#[cfg(test)]
fn encode(manifest: &toml::Value, roles: &[Role]) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&fingerprint(manifest, roles))?)
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
        assert_eq!(bundle_digest(&manifest, &[]).unwrap(), digest(&bytes));
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
        let bytes = encode(&manifest, std::slice::from_ref(&role)).unwrap();
        assert_eq!(bundle_digest(&manifest, &[role]).unwrap(), digest(&bytes));
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
            for result in [
                encode(&value, &[]).map(|_| ()),
                bundle_digest(&value, &[]).map(|_| ()),
            ] {
                assert!(result.unwrap_err().to_string().contains("非有限"));
            }
        }
    }
    #[test]
    fn numeric_boundaries_and_fractional_dates_have_fixed_bytes_and_hash() {
        let manifest: toml::Value = toml::from_str(
            "negative_zero=-0.0\nfloat_min=5e-324\nfloat_max=1.7976931348623157e308\ninteger_min=-9223372036854775808\ninteger_max=9223372036854775807\nlocal=1979-05-27 07:32:00.123456789\noffset=1979-05-27T07:32:00.000000001+08:00\ntime=07:32:00.125\n"
        ).unwrap();
        let expected = r#"{"algorithm":"sha256-json-v3","manifest":{"float_max":1.7976931348623157e+308,"float_min":5e-324,"integer_max":9223372036854775807,"integer_min":-9223372036854775808,"local":"1979-05-27T07:32:00.123456789","negative_zero":-0.0,"offset":"1979-05-27T07:32:00.000000001+08:00","time":"07:32:00.125"},"roles":[]}"#;
        let bytes = encode(&manifest, &[]).unwrap();
        assert_eq!(bytes, expected.as_bytes());
        assert_eq!(
            digest(&bytes),
            "179665fed1fc70eb02967c3cda224cc923c26d120760ca62f080825b8e3727bf"
        );
        assert_eq!(
            bundle_digest(&manifest, &[]).unwrap(),
            "179665fed1fc70eb02967c3cda224cc923c26d120760ca62f080825b8e3727bf"
        );
    }

    #[test]
    fn role_order_is_canonical_but_mode_arrays_keep_their_order() {
        let manifest = toml::from_str("[metadata]\nz=1\na=2\n").unwrap();
        let role = |name: &str| Role {
            name: name.into(),
            file: format!("agents/{name}.toml"),
            modes: vec!["z".into(), "a".into()],
            access: "task-scoped".into(),
            model: Some("model".into()),
            reasoning_effort: Some("high".into()),
            sha256: "file".into(),
            prompt_sha256: "prompt".into(),
        };
        let expected = r#"{"algorithm":"sha256-json-v3","manifest":{"metadata":{"a":2,"z":1}},"roles":[{"access":"task-scoped","file":"agents/a.toml","model":"model","modes":["z","a"],"name":"a","prompt_sha256":"prompt","reasoning_effort":"high","sha256":"file"},{"access":"task-scoped","file":"agents/z.toml","model":"model","modes":["z","a"],"name":"z","prompt_sha256":"prompt","reasoning_effort":"high","sha256":"file"}]}"#;
        let reverse = [role("z"), role("a")];
        assert_eq!(encode(&manifest, &reverse).unwrap(), expected.as_bytes());
        assert_eq!(
            bundle_digest(&manifest, &reverse).unwrap(),
            "d7dbccfb6bd8b0f8c8583b017031f92afad56fa2e03dfb58b6339ef8e8a8d28a"
        );
        assert_eq!(
            encode(&manifest, &[role("a"), role("z")]).unwrap(),
            expected.as_bytes()
        );
        let reordered = toml::from_str("[metadata]\na=2\nz=1\n").unwrap();
        assert_eq!(
            bundle_digest(&reordered, &reverse).unwrap(),
            "d7dbccfb6bd8b0f8c8583b017031f92afad56fa2e03dfb58b6339ef8e8a8d28a"
        );
    }
}
