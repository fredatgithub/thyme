use std::{fs, path::Path};

use chia::{
    protocol::Coin,
    puzzles::{EveProof, LineageProof, Proof},
};
use chia_wallet_sdk::Cat;
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};
use serde_with::{hex::Hex, serde_as};

#[serde_as]
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Derivations {
    pub previous_height: Option<u32>,
    #[serde_as(as = "Hex")]
    pub header_hash: [u8; 32],
    #[serde_as(as = "IndexSet<Hex>")]
    pub puzzle_hashes: IndexSet<[u8; 32]>,
    #[serde_as(as = "IndexMap<Hex, _>")]
    pub coin_states: IndexMap<[u8; 32], CoinStateJson>,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageProofJson {
    #[serde_as(as = "Hex")]
    pub parent_parent_coin_info: [u8; 32],
    #[serde_as(as = "Hex")]
    pub parent_inner_puzzle_hash: [u8; 32],
    pub parent_amount: u64,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EveProofJson {
    #[serde_as(as = "Hex")]
    pub parent_parent_coin_info: [u8; 32],
    pub parent_amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProofJson {
    Lineage(LineageProofJson),
    Eve(EveProofJson),
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinJson {
    #[serde_as(as = "Hex")]
    pub parent_coin_info: [u8; 32],
    #[serde_as(as = "Hex")]
    pub puzzle_hash: [u8; 32],
    pub amount: u64,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinStateJson {
    pub coin: CoinJson,
    pub parent_puzzle: Option<PuzzleInfo>,
    pub created_height: Option<u32>,
    pub spent_height: Option<u32>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PuzzleInfo {
    Cat(CatJson),
    Unknown,
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatJson {
    #[serde_as(as = "Hex")]
    pub asset_id: [u8; 32],
    #[serde_as(as = "Hex")]
    pub p2_puzzle_hash: [u8; 32],
    pub coin: CoinJson,
    pub lineage_proof: Option<LineageProofJson>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Cache {
    pub derivations: Vec<Derivations>,
}

impl Cache {
    pub fn load(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            let cache = Self::default();
            cache.save(path)?;
            return Ok(cache);
        }
        let contents = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&contents)?)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let contents = serde_json::to_string_pretty(self)?;
        fs::write(path, contents)?;
        Ok(())
    }
}

impl From<Coin> for CoinJson {
    fn from(value: Coin) -> Self {
        Self {
            parent_coin_info: value.parent_coin_info.into(),
            puzzle_hash: value.puzzle_hash.into(),
            amount: value.amount,
        }
    }
}

impl From<CoinJson> for Coin {
    fn from(value: CoinJson) -> Self {
        Self {
            parent_coin_info: value.parent_coin_info.into(),
            puzzle_hash: value.puzzle_hash.into(),
            amount: value.amount,
        }
    }
}

impl From<LineageProof> for LineageProofJson {
    fn from(value: LineageProof) -> Self {
        Self {
            parent_parent_coin_info: value.parent_parent_coin_info.into(),
            parent_inner_puzzle_hash: value.parent_inner_puzzle_hash.into(),
            parent_amount: value.parent_amount,
        }
    }
}

impl From<LineageProofJson> for LineageProof {
    fn from(value: LineageProofJson) -> Self {
        Self {
            parent_parent_coin_info: value.parent_parent_coin_info.into(),
            parent_inner_puzzle_hash: value.parent_inner_puzzle_hash.into(),
            parent_amount: value.parent_amount,
        }
    }
}

impl From<EveProof> for EveProofJson {
    fn from(value: EveProof) -> Self {
        Self {
            parent_parent_coin_info: value.parent_parent_coin_info.into(),
            parent_amount: value.parent_amount,
        }
    }
}

impl From<EveProofJson> for EveProof {
    fn from(value: EveProofJson) -> Self {
        Self {
            parent_parent_coin_info: value.parent_parent_coin_info.into(),
            parent_amount: value.parent_amount,
        }
    }
}

impl From<Proof> for ProofJson {
    fn from(value: Proof) -> Self {
        match value {
            Proof::Lineage(proof) => Self::Lineage(proof.into()),
            Proof::Eve(proof) => Self::Eve(proof.into()),
        }
    }
}

impl From<ProofJson> for Proof {
    fn from(value: ProofJson) -> Self {
        match value {
            ProofJson::Lineage(proof) => Self::Lineage(proof.into()),
            ProofJson::Eve(proof) => Self::Eve(proof.into()),
        }
    }
}

impl From<Cat> for CatJson {
    fn from(value: Cat) -> Self {
        Self {
            asset_id: value.asset_id.into(),
            p2_puzzle_hash: value.p2_puzzle_hash.into(),
            coin: value.coin.into(),
            lineage_proof: value.lineage_proof.map(Into::into),
        }
    }
}
