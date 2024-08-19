use std::{fs, path::Path};

use hex_literal::hex;
use serde::{Deserialize, Serialize};
use serde_with::{hex::Hex, serde_as};

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub full_node_uri: String,
    #[serde_as(as = "Hex")]
    pub genesis_challenge: [u8; 32],
    pub network_id: String,
    pub dust_threshold: u64,
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            let config = Self::default();
            config.save(path)?;
            return Ok(config);
        }
        let contents = fs::read_to_string(path)?;
        Ok(toml::from_str(&contents)?)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let contents = toml::to_string_pretty(self)?;
        fs::write(path, contents)?;
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            full_node_uri: "localhost:8444".to_string(),
            genesis_challenge: hex!(
                "ccd5bb71183532bff220ba46c268991a3ff07eb358e8255a65c30a2dce0e5fbb"
            ),
            network_id: "mainnet".to_string(),
            dust_threshold: 0,
        }
    }
}
