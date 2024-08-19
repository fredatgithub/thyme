use anyhow::bail;
use chia::{
    client::Peer,
    protocol::{
        Bytes32, CoinState, CoinStateFilters, RejectPuzzleState, RejectStateReason,
        RequestPuzzleState, RespondPuzzleState,
    },
};

pub async fn fetch_coin_states(
    peer: &Peer,
    genesis_challenge: Bytes32,
    mut start_previous_height: Option<u32>,
    start_header_hash: Bytes32,
    puzzle_hashes: impl IntoIterator<Item = impl Into<Bytes32>>,
    dust_threshold: u64,
) -> anyhow::Result<(Vec<CoinState>, u32, Bytes32)> {
    let mut previous_height = start_previous_height;
    let mut header_hash = start_header_hash;
    let mut coin_states = Vec::new();

    let puzzle_hashes = puzzle_hashes
        .into_iter()
        .map(Into::into)
        .collect::<Vec<_>>();

    loop {
        let response: Result<RespondPuzzleState, chia::client::Error<RejectPuzzleState>> = peer
            .request_or_reject(RequestPuzzleState {
                puzzle_hashes: puzzle_hashes.clone(),
                previous_height,
                header_hash,
                filters: CoinStateFilters::new(true, true, true, 0),
                subscribe_when_finished: false,
            })
            .await;

        match response {
            Ok(response) => {
                coin_states.extend(response.coin_states);
                previous_height = Some(response.height);
                header_hash = response.header_hash;
                if response.is_finished {
                    break;
                }
            }
            Err(chia::client::Error::Rejection(rejection)) => match rejection.reason {
                RejectStateReason::ExceededSubscriptionLimit => {
                    bail!("Exceeded subscription limit even though we didn't subscribe.");
                }
                RejectStateReason::Reorg => {
                    if start_previous_height.is_none() {
                        bail!("Reorg detected but we didn't specify a previous height.");
                    }
                    start_previous_height = None;
                    previous_height = None;
                    header_hash = genesis_challenge;
                    coin_states.clear();
                }
            },
            Err(error) => bail!(error),
        }
    }

    let coin_states = coin_states
        .into_iter()
        .filter(|cs| cs.coin.amount >= dust_threshold)
        .collect();

    Ok((coin_states, previous_height.unwrap(), header_hash))
}
