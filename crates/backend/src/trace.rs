use std::{io::Read, process::Stdio};

use alloy_consensus::{
    EthereumTxEnvelope, TxEip4844Variant,
    transaction::{Recovered, SignerRecoverable},
};
use alloy_network::{AnyTxEnvelope, TransactionResponse};
use alloy_primitives::TxHash;
use alloy_provider::Provider;
use alloy_rpc_client::RpcClient;
use alloy_rpc_types::{BlockTransactions, transaction::Transaction};
use alloy_rpc_types_trace::parity::TransactionTrace;
use eyre::Result;
use foundry_evm::{
    Env,
    core::env::AsEnvMut,
    executors::TracingExecutor,
    opts::EvmOpts,
    traces::{CallTraceArena, TraceMode},
    utils::configure_tx_env,
};
use futures::TryFutureExt;
use tracing::instrument;
use url::Url;

pub use crate::convert::ParityTraces;

// TODO: need to fix this
/// Replays a transaction and returns the trace arena data
/// NOTE: copied from foundry
#[allow(dead_code)]
#[instrument(skip(provider, url))]
pub async fn get_transaction_trace(
    provider: impl Provider,
    url: &Url,
    tx_hash: TxHash,
    replay: bool,
) -> Result<CallTraceArena> {
    let evm_opts = EvmOpts {
        fork_url: Some(url.to_string()),
        ..Default::default()
    };
    let tx = provider
        .get_transaction_by_hash(tx_hash)
        .await?
        .ok_or_else(|| eyre::eyre!("tx not found: {:?}", tx_hash))?;
    tracing::debug!("got tx by hash");

    let tx_block_number = tx
        .block_number
        .ok_or_else(|| eyre::eyre!("tx may still be pending: {:?}", tx_hash))?;

    let mut config = foundry_config::Config {
        fork_block_number: Some(tx_block_number - 1),
        eth_rpc_url: Some(url.to_string()),
        ..Default::default()
    };

    let create2_deployer = evm_opts.create2_deployer;
    let (block, (env, fork, _chain, networks)) = tokio::try_join!(
        provider
            .get_block(tx_block_number.into())
            .full()
            .into_future()
            .map_err(Into::into),
        TracingExecutor::get_fork_material(&mut config, evm_opts)
    )?;
    tracing::debug!("got block and fork material");

    let trace_mode = TraceMode::Call;
    let mut executor = TracingExecutor::new(
        env.clone(),
        fork,
        None,
        trace_mode,
        networks,
        create2_deployer,
        None,
    )?;
    tracing::debug!("created executor");

    let mut env = Env::new_with_spec_id(
        env.evm_env.cfg_env.clone(),
        env.evm_env.block_env.clone(),
        env.tx.clone(),
        executor.spec_id(),
    );
    tracing::debug!("created env");

    // Replay previous transactions in the block
    if let Some(block) = block
        && replay
    {
        tracing::debug!("replaying previous txs");
        let BlockTransactions::Full(ref txs) = block.transactions else {
            return Err(eyre::eyre!("Could not get block txs"));
        };

        for (idx, tx_in_block) in txs.iter().enumerate() {
            tracing::debug!("executing tx {idx}/{}", block.transactions.len());
            if tx_in_block.tx_hash() == tx_hash {
                break;
            }

            let tx = ethereum_tx_to_any_tx(tx_in_block)?;
            configure_tx_env(&mut env.as_env_mut(), &tx);
            // env.evm_env.cfg_env.disable_balance_check = true;

            if alloy_consensus::Transaction::to(tx_in_block).is_some() {
                let _ = executor.transact_with_env(env.clone());
            } else {
                let _ = executor.deploy_with_env(env.clone(), None);
            }
        }
    }

    tracing::debug!("executing target tx");
    // Execute target transaction
    configure_tx_env(&mut env.as_env_mut(), &ethereum_tx_to_any_tx(&tx)?);
    // dont need this in real txs
    // if is_impersonated_tx(tx.inner.inner.inner()) {
    //     env.evm_env.cfg_env.disable_balance_check = true;
    // }

    let result = if alloy_consensus::Transaction::to(&tx).is_some() {
        executor.transact_with_env(env)?
    } else {
        executor.deploy_with_env(env, None)?.raw
    };

    Ok(result
        .traces
        .ok_or_else(|| eyre::eyre!("No traces found"))?
        .arena)
}

fn ethereum_tx_to_any_tx(
    tx: &Transaction<EthereumTxEnvelope<TxEip4844Variant>>,
) -> eyre::Result<Transaction<AnyTxEnvelope>> {
    let recovered = tx.inner.recover_signer()?;
    let tx = Transaction {
        inner: Recovered::new_unchecked(
            AnyTxEnvelope::Ethereum(tx.inner.clone().into_inner()),
            recovered,
        ),
        block_hash: tx.block_hash,
        block_number: tx.block_number,
        transaction_index: tx.transaction_index,
        effective_gas_price: tx.effective_gas_price,
    };
    Ok(tx)
}

#[instrument(skip(url))]
pub async fn get_transaction_trace_cast(
    url: &Url,
    tx_hash: TxHash,
    replay: bool,
) -> Result<CallTraceArena> {
    let mut cmd = std::process::Command::new("cast");

    cmd.args([
        "run",
        &tx_hash.to_string(),
        "--rpc-url",
        url.as_str(),
        "--json",
    ]);

    if !replay {
        cmd.arg("--quick");
    }

    cmd.env_clear();
    if let Ok(path) = std::env::var("PATH") {
        cmd.env("PATH", path);
    }

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    tracing::debug!("spawning cast run");
    let mut child = cmd.spawn()?;

    // Take stdout for streaming, stderr for error capture
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| eyre::eyre!("No stdout"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| eyre::eyre!("No stderr"))?;

    // Stream JSON directly from stdout without buffering entire output
    // This happens while the process is still running
    let parse_result = serde_json::from_reader(stdout);

    // Now wait for process to complete
    let status = child.wait()?;
    tracing::debug!("cast run status: {:?}", status.code());

    // Handle the parse result and process status
    match (parse_result, status.success()) {
        (Ok(data), true) => Ok(data),
        (Err(e), true) => {
            tracing::error!("JSON parse error despite successful exit: {e}");
            Err(e.into())
        }
        (_, false) => {
            let mut err_string = String::new();
            stderr.read_to_string(&mut err_string)?;
            tracing::error!("cast run failed: {:?} err: {err_string}", status.code());
            eyre::bail!("cast run failed: {err_string}")
        }
    }
}

async fn get_transaction_trace_rpc(opts: &TraceOpts<'_>) -> Result<CallTraceArena> {
    let rpc_client = RpcClient::new_http(opts.url.clone());
    let trace = rpc_client
        .request::<(TxHash,), Vec<TransactionTrace>>("trace_transaction", (opts.tx_hash,))
        .await?;
    ParityTraces(trace).try_into()
}

pub enum Strategy {
    LocalCastRun,
    TraceTransactionRpc,
}

pub struct TraceOpts<'a> {
    url: &'a Url,
    tx_hash: TxHash,
    replay: bool,
}

impl<'a> TraceOpts<'a> {
    pub fn new(url: &'a Url, tx_hash: TxHash, replay: bool) -> Self {
        Self {
            url,
            tx_hash,
            replay,
        }
    }

    pub async fn get_trace(&self, strategy: Strategy) -> Result<CallTraceArena> {
        match strategy {
            Strategy::LocalCastRun => {
                get_transaction_trace_cast(self.url, self.tx_hash, self.replay).await
            }
            Strategy::TraceTransactionRpc => get_transaction_trace_rpc(self).await,
        }
    }
}
