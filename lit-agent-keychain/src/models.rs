use crate::{
    config::{validate_origin, Config},
    crypto,
};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Owner {
    Wallet {
        address: String,
    },
    #[serde(rename_all = "camelCase")]
    Passkey {
        public_key: String,
        credential_id: String,
        rp_id: String,
        origin: String,
    },
    #[serde(rename_all = "camelCase")]
    Google {
        subject: String,
        client_id: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Authority {
    pub v: u8,
    pub network: String,
    pub registry: String,
    pub owner: Owner,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub v: u8,
    pub network: String,
    pub registry: String,
    pub vault_id: String,
    pub authority_cid: String,
    pub secret_id: String,
    pub release: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReceiptPayload {
    pub v: u8,
    pub domain: String,
    pub vault_id: String,
    pub object_hash: String,
    pub issued_at: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub payload: ReceiptPayload,
    pub signature: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Signed {
    pub document: Value,
    pub receipt: Receipt,
}
impl Authority {
    pub fn vault_id(&self) -> Result<String> {
        crypto::digest(&serde_json::to_value(self)?)
    }
    pub fn validate(&self, cfg: &Config) -> Result<()> {
        if self.v != 2 || self.network != cfg.network || self.registry != cfg.public_base_url {
            bail!("authority domain mismatch");
        }
        match &self.owner {
            Owner::Wallet { address } => {
                if !address.starts_with("0x") || !valid_hex(&address[2..], 20) {
                    bail!("invalid wallet address");
                }
            }
            Owner::Passkey {
                public_key,
                credential_id,
                rp_id,
                origin,
            } => {
                validate_origin(origin)?;
                let url = reqwest::Url::parse(origin)?;
                let hostname = url.host_str().unwrap_or_default();
                if !valid_hex(public_key, 65)
                    || !public_key.starts_with("04")
                    || credential_id.is_empty()
                    || credential_id.len() > 1400
                    || !credential_id
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
                    || !(hostname == rp_id || hostname.ends_with(&format!(".{rp_id}")))
                {
                    bail!("invalid passkey");
                }
            }
            Owner::Google { subject, client_id } => {
                if subject.is_empty()
                    || subject.len() > 255
                    || Some(client_id) != cfg.google_client_id.as_ref()
                {
                    bail!("invalid Google owner");
                }
            }
        }
        Ok(())
    }
}
impl Manifest {
    pub fn validate(&self, cfg: &Config) -> Result<()> {
        if self.v != 2
            || self.network != cfg.network
            || self.registry != cfg.public_base_url
            || !valid_hex(&self.vault_id, 32)
            || !valid_hex(&self.secret_id, 32)
            || !valid_cid(&self.authority_cid)
            || !matches!(self.release.as_str(), "export" | "stripe_balance")
        {
            bail!("invalid manifest");
        }
        Ok(())
    }
}
pub fn valid_hex(s: &str, bytes: usize) -> bool {
    s.len() == bytes * 2
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
pub fn valid_cid(s: &str) -> bool {
    s.len() == 46
        && s.starts_with("Qm")
        && s.bytes()
            .all(|c| b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".contains(&c))
}
pub fn field<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing string {key}"))
}
pub fn number(v: &Value, key: &str) -> Result<i64> {
    v.get(key)
        .and_then(Value::as_i64)
        .filter(|v| (0..=9_007_199_254_740_991).contains(v))
        .ok_or_else(|| anyhow::anyhow!("invalid number {key}"))
}
