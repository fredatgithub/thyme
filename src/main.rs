use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail};
use cache::{Cache, CoinStateJson, Derivations, PuzzleInfo};
use chia::{
    bls::{master_to_wallet_unhardened_intermediate, DerivableKey, PublicKey},
    client::Peer,
    clvm_traits::ToClvm,
    protocol::{
        NodeType, PuzzleSolutionResponse, RejectCoinState, RejectPuzzleSolution, RequestCoinState,
        RespondCoinState,
    },
    puzzles::{standard::StandardArgs, DeriveSynthetic},
};
use chia_wallet_sdk::{connect_peer, create_tls_connector, load_ssl_cert, Cat, Primitive, Puzzle};
use chrono::{Local, TimeZone};
use clap::Parser;
use clvmr::Allocator;
use config::Config;
use fetch::fetch_coin_states;
use indexmap::IndexMap;
use rayon::iter::{IntoParallelIterator, ParallelIterator};

mod cache;
mod config;
mod fetch;

/// Generates a CSV file with observer key Chia transaction info for a given tax year.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// The master public key of the wallet to lookup transactions for.
    #[arg(short, long)]
    key: String,

    /// The year you are interested in, from Jan 1st to Dec 31st, inclusive.
    #[arg(short, long)]
    year: i32,

    /// Whether to reset the cache before running.
    #[arg(short, long)]
    reset: bool,

    /// The dust threshold to filter out small transactions. Defaults to 0.
    #[arg(short, long)]
    dust_threshold: Option<u64>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse arguments and setup timezone and key info.
    let args = Args::parse();
    let local_timezone = Local::now().timezone();
    let master_pk = parse_pk(&args.key)?;
    let intermediate_pk = master_to_wallet_unhardened_intermediate(&master_pk);
    let fingerprint = master_pk.get_fingerprint();

    // Load the config and cache.
    let cache_dir = PathBuf::from("cache");
    if !cache_dir.try_exists()? {
        fs::create_dir_all(cache_dir.as_path())?;
    }
    let cache_path = cache_dir.join(format!("cache-{fingerprint}-{}.json", args.year));
    let config_path = "config.toml";

    let config = Config::load(config_path)?;
    let mut cache = Cache::load(cache_path.as_path())?;

    // Setup January 1st of the year and the next year.
    let start_date = local_timezone
        .with_ymd_and_hms(args.year, 1, 1, 0, 0, 0)
        .unwrap()
        .timestamp();

    let end_date = local_timezone
        .with_ymd_and_hms(args.year + 1, 1, 1, 0, 0, 0)
        .unwrap()
        .timestamp();

    // Create and load an SSL certificate and connect to the peer.
    let cert = load_ssl_cert("thyme.crt", "thyme.key")?;
    let tls_connector = create_tls_connector(&cert)?;
    let peer = connect_peer(&config.full_node_uri, tls_connector).await?;
    peer.send_handshake(config.network_id.clone(), NodeType::Wallet)
        .await?;

    update_cache(&mut cache, cache_path, &config, &peer, &intermediate_pk).await?;

    // Do something with the cached and saved coin data.

    Ok(())
}

async fn update_cache(
    cache: &mut Cache,
    cache_path: impl AsRef<Path>,
    config: &Config,
    peer: &Peer,
    intermediate_pk: &PublicKey,
) -> anyhow::Result<()> {
    let cache_path = cache_path.as_ref();
    let mut index = 0;

    loop {
        println!(
            "Fetching coin states starting from derivation {}",
            index * 1000
        );

        if cache.derivations.len() <= index {
            let start = index as u32 * 1000;
            cache.derivations.push(Derivations {
                previous_height: None,
                header_hash: config.genesis_challenge,
                puzzle_hashes: (start..=start + 1000)
                    .into_par_iter()
                    .map(|i| {
                        let pk = intermediate_pk.derive_unhardened(i).derive_synthetic();
                        StandardArgs::curry_tree_hash(pk).to_bytes()
                    })
                    .collect::<Vec<_>>()
                    .into_iter()
                    .collect(),
                coin_states: IndexMap::new(),
            });

            cache.save(cache_path)?;
        }

        let (coin_states, previous_height, previous_header_hash) = fetch_coin_states(
            peer,
            config.genesis_challenge.into(),
            cache.derivations[index].previous_height,
            cache.derivations[index].header_hash.into(),
            cache.derivations[index].puzzle_hashes.clone(),
            config.dust_threshold,
        )
        .await?;

        let len = coin_states.len();

        for (i, coin_state) in coin_states.into_iter().enumerate() {
            let parent_puzzle = if cache.derivations[index]
                .puzzle_hashes
                .contains(&coin_state.coin.puzzle_hash.to_bytes())
            {
                None
            } else {
                if let Some(existing) = cache.derivations[index]
                    .coin_states
                    .get(&coin_state.coin.coin_id().to_bytes())
                    .cloned()
                {
                    if existing.spent_height == coin_state.spent_height {
                        println!("Skipping existing coin {}", coin_state.coin.coin_id());
                        continue;
                    }
                }

                println!(
                    "Fetching puzzle data for parent coin {} ({}/{})",
                    coin_state.coin.parent_coin_info, i, len,
                );

                let response: Result<
                    PuzzleSolutionResponse,
                    chia::client::Error<RejectPuzzleSolution>,
                > = peer
                    .request_puzzle_and_solution(
                        coin_state.coin.parent_coin_info,
                        coin_state.created_height.unwrap(),
                    )
                    .await;

                match response {
                    Ok(response) => {
                        let csr: RespondCoinState = peer
                            .request_or_reject::<_, RejectCoinState, _>(RequestCoinState {
                                coin_ids: vec![coin_state.coin.parent_coin_info],
                                previous_height: None,
                                header_hash: config.genesis_challenge.into(),
                                subscribe: false,
                            })
                            .await?;

                        let Some(parent_coin_state) = csr.coin_states.into_iter().next() else {
                            bail!(
                                "Parent coin state not found with id {}",
                                coin_state.coin.parent_coin_info
                            );
                        };

                        let mut allocator = Allocator::new();
                        let puzzle_ptr = response.puzzle.to_clvm(&mut allocator)?;
                        let parent_puzzle = Puzzle::parse(&allocator, puzzle_ptr);
                        let parent_solution = response.solution.to_clvm(&mut allocator)?;

                        Cat::from_parent_spend(
                            &mut allocator,
                            parent_coin_state.coin,
                            parent_puzzle,
                            parent_solution,
                            coin_state.coin,
                        )
                        .ok()
                        .flatten()
                        .map(|cat| PuzzleInfo::Cat(cat.into()))
                    }
                    Err(chia::client::Error::Rejection(_rejection)) => None,
                    Err(error) => {
                        return Err(error.into());
                    }
                }
            };

            cache.derivations[index].coin_states.insert(
                coin_state.coin.coin_id().into(),
                CoinStateJson {
                    coin: coin_state.coin.into(),
                    parent_puzzle,
                    created_height: coin_state.created_height,
                    spent_height: coin_state.spent_height,
                },
            );
            cache.save(cache_path)?;
        }

        cache.derivations[index].previous_height = Some(previous_height);
        cache.derivations[index].header_hash = previous_header_hash.into();
        cache.save(cache_path)?;

        if cache.derivations[index].coin_states.is_empty() {
            break;
        }

        index += 1;
    }

    Ok(())
}

fn parse_pk(pk: &str) -> anyhow::Result<PublicKey> {
    let trimmed = pk.trim();
    let stripped = if let Some(after) = trimmed.strip_prefix("0x") {
        after
    } else {
        trimmed
    };
    let bytes = hex::decode(stripped)?;
    let array = bytes
        .try_into()
        .map_err(|_| anyhow!("Public key is not 48 bytes long"))?;
    Ok(PublicKey::from_bytes(&array)?)
}
