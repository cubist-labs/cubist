//! Ethereum credential handling

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{collections::BTreeMap, sync::Arc};

use cubist_config::network::CredConfig;
use ethers::providers::{Http, Middleware, Provider, ProviderError};
use ethers::signers::LocalWallet;
use ethers::types::{transaction::eip2718::TypedTransaction, Signature};
use ethers::types::{BlockNumber, TransactionRequest, U256};
use ethers::utils::rlp::Rlp;
use ethers::{
    abi::Address,
    prelude::k256::ecdsa::SigningKey,
    signers::{coins_bip39::English, MnemonicBuilder, Signer, Wallet},
};
use futures::{SinkExt, TryStreamExt};
use hyper::Uri;
use secrecy::ExposeSecret;
use serde::Deserialize;
use serde_json::{json, to_value, Value};

use crate::jrpc::IdReq;

use crate::{
    connector::{connect, ConcretePair},
    jrpc::{self, response, ErrorCode},
    FatalErr, JrpcRequest, JsonRpcErr, Pair,
};

use super::switch;

/// Build wallets for a given [`CredConfig`].
fn to_wallets(cred: &CredConfig) -> Result<Vec<LocalWallet>, FatalErr> {
    fn to_fatal(e: cubist_config::ConfigError) -> FatalErr {
        FatalErr::ReadSecretError(format!("{e}"))
    }

    match cred {
        // mnemonic
        CredConfig::Mnemonic(m) => {
            let mn = m.seed.load().map_err(to_fatal)?;
            let mut builder =
                MnemonicBuilder::<English>::default().phrase(mn.expose_secret().as_str());
            let mut out = Vec::with_capacity(m.account_count as usize);
            for i in 0..m.account_count {
                builder = builder.derivation_path(format!("{}{i}", m.derivation_path).as_str())?;
                out.push(builder.build()?);
            }
            Ok(out)
        }
        // keystore
        CredConfig::Keystore(ks) => {
            let password = ks.password.load().map_err(to_fatal)?;
            let wallet =
                Wallet::<SigningKey>::decrypt_keystore(&ks.file, password.expose_secret())?;
            Ok(vec![wallet])
        }
        // private key
        CredConfig::PrivateKey(pk) => {
            let private_key = pk.hex.load().map_err(to_fatal)?;
            let wallet = LocalWallet::from_str(private_key.expose_secret())?;
            Ok(vec![wallet])
        }
    }
}

/// Build wallets for a number of [`CredConfig`]s.
pub fn build_wallets<'a>(
    creds: impl Iterator<Item = &'a CredConfig>,
) -> Result<Vec<LocalWallet>, FatalErr> {
    let wallets = creds
        .map(to_wallets)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect();
    Ok(wallets)
}

/// Determines what credentials the eth_creds transformer is aware of.
#[derive(Debug, Clone)]
pub struct EthProxyConfig {
    /// Credentials configuration
    pub creds: Vec<CredConfig>,

    /// The URI to use for querying on-chain state. Used in nonce generation.
    pub onchain_uri: Option<Uri>,

    /// Chain id (transaction chain ID must be set before signing)
    pub chain_id: u32,
}

const ETH_SEND_TRANSACTION: &str = "eth_sendTransaction";

/// Represents the possible credential-related eth_* method that `cred_handler` can handle
enum CredMethod {
    /// `eth_accounts`
    Accounts,
    /// `eth_signTransaction`
    SignTransaction,
}

impl CredMethod {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "eth_accounts" => Some(Self::Accounts),
            "eth_signTransaction" => Some(Self::SignTransaction),
            _ => None,
        }
    }
}

#[derive(Debug)]
/// User accounts (wallets) for a particular chain.
pub struct Accounts {
    pub(crate) wallets: BTreeMap<Address, LocalWallet>,
}

impl Accounts {
    /// Builds a local wallet from a number of configured cred sources.
    /// The output is a BTreeMap to ensure a stable ordering and allow random lookup.
    pub fn from_cfg(cfg: &EthProxyConfig) -> Result<Self, FatalErr> {
        Ok(Self {
            wallets: build_wallets(cfg.creds.iter())?
                .into_iter()
                .map(|w| (w.address().to_owned(), w.with_chain_id(cfg.chain_id)))
                .collect(),
        })
    }
}

/// `EthProxy` can wrap around a `Pair`, intercepting and transforming traffic
/// related to credentials.
pub struct CredProxy {
    provider: Option<Provider<Http>>,
    nonce_counter: HashMap<Address, AtomicU64>,
    accounts: Accounts,
}

impl CredProxy {
    /// Instantiates a proxy using the provided config, including account generation
    pub fn from_cfg(cfg: &EthProxyConfig) -> Result<Self, FatalErr> {
        let accounts = Accounts::from_cfg(cfg)?;

        // We need force the scheme to be Http because ethers providers are not dyn safe
        // so we just always use http
        let uri = cfg.onchain_uri.clone().map(|uri| {
            let scheme = match &uri.scheme_str() {
                Some("http" | "ws") => "http".parse().unwrap(),
                _ => "https".parse().unwrap(),
            };

            let mut parts = uri.into_parts();
            parts.scheme = Some(scheme);
            Uri::from_parts(parts).unwrap()
        });
        let provider = uri
            .as_ref()
            .map(Uri::to_string)
            .map(Provider::try_from)
            .transpose()?;
        let nonce_counter = accounts
            .wallets
            .iter()
            .map(|(addr, _)| (*addr, AtomicU64::new(0)))
            .collect();

        Ok(Self {
            accounts,
            provider,
            nonce_counter,
        })
    }

    /// Handles ethereum requests that pertain to credentials
    pub fn wrap(
        self: &Arc<CredProxy>,
        endpoint: impl Pair<Value, JrpcRequest> + 'static,
    ) -> impl Pair<Value, JrpcRequest> {
        let (handled, pass) = switch(endpoint, |res| {
            res.as_ref()
                .ok()
                .map(|req| CredMethod::from_str(&req.method).is_some())
                .unwrap_or(false)
        });

        let pair = self.handler_pair();
        tokio::spawn(async move {
            if let Err(e) = connect(handled, pair).await {
                tracing::debug!("Shutting down credential handler with error: {e}")
            }
        });

        // Unlike the other credmethods, which we handle entirely within the proxy,
        // we translate eth_sendTransaction into eth_sendRawTransaction and pass it along
        // to the onchain endpoint.
        let proxy = self.clone();

        pass.and_then(move |req: JrpcRequest| {
            let proxy = proxy.clone();
            async move {
                // This operation is a no-op for all other requests
                if req.method != ETH_SEND_TRANSACTION {
                    tracing::debug!("passing through: {req}");
                    return Ok(req);
                }

                tracing::debug!("Converting sendTransaction to sendRawTransaction: {req}...");
                let send = proxy.eth_send_transaction(req).await;
                tracing::debug!("Converted transaction");
                send
            }
        })
    }

    fn handler_pair(self: &Arc<CredProxy>) -> impl Pair<JrpcRequest, Value> {
        let ref_self = self.clone();
        ConcretePair::pipe()
            .sink_err_into()
            .and_then(move |req: JrpcRequest| {
                let ref_self = ref_self.clone();
                async move {
                    match CredMethod::from_str(&req.method)
                        .expect("cred_handler should only be invoked with cred methods")
                    {
                        CredMethod::Accounts => ref_self.eth_accounts(req),
                        CredMethod::SignTransaction => ref_self.eth_sign_transaction(req).await,
                    }
                }
            })
    }

    async fn nonce_for_account(&self, a: Address) -> Result<U256, ProviderError> {
        let next_available = &self.nonce_counter[&a];

        let load_and_increment = || Ok(next_available.fetch_add(1u64, Ordering::SeqCst).into());

        // The normal case
        if next_available.load(Ordering::SeqCst) > 0 {
            return load_and_increment();
        }

        // If we don't have a provider, just start counting from zero
        // This is useful in testing
        let provider = if let Some(provider) = &self.provider {
            provider
        } else {
            return Ok(0.into());
        };

        // If the value is equal to zero (uninitialized), now we're racing.
        // That is, multiple threads could have simultaneously failed the check above.
        // This request could be duplicated
        let trx_count = provider
            .get_transaction_count(a, Some(BlockNumber::Pending.into()))
            .await?;

        // This is racy but it's better than blowing up?
        if trx_count >= U256::from(u64::MAX) {
            return Ok(trx_count);
        }

        let trx_count64 = trx_count.as_u64();

        // If this write succeeds then we are the initializer, we won the race
        let winner = next_available
            // We add 1 to trx_count64 because we're about to use trx_count as a nonce
            // so the next available is that plus 1.
            .compare_exchange(0, trx_count64 + 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok();

        if winner {
            return Ok(trx_count);
        }

        // We lost the race and someone else has populated the nonce, let's just add one to it.
        load_and_increment()
    }

    /// Responds to the `eth_accounts` request by enumerating all wallets
    fn eth_accounts(&self, req: JrpcRequest) -> Result<Value, JsonRpcErr> {
        let accounts: Vec<_> = self
            .accounts
            .wallets
            .values()
            .map(Signer::address)
            .collect();
        response(req.id, json!(accounts))
    }

    async fn sign_transaction(
        &self,
        id: IdReq,
        mut transaction: TypedTransaction,
    ) -> Result<(Signature, TypedTransaction), JsonRpcErr> {
        let from_address = *transaction.from().ok_or_else(|| {
            jrpc::error(
                ErrorCode::InvalidParams,
                "'from' field is required",
                id.clone(),
                "",
            )
        })?;

        let wallet = self.accounts.wallets.get(&from_address).ok_or_else(|| {
            jrpc::error(
                ErrorCode::InvalidRequest,
                format!("No wallet found for sender '{}'", from_address),
                id.clone(),
                "",
            )
        })?;

        tracing::debug!("Setting chain id to: {}", wallet.chain_id());
        transaction.set_chain_id(wallet.chain_id());

        // If the offchain client has not provided a nonce, we need to set one
        match transaction.nonce() {
            None => {
                tracing::debug!("Setting nonce for trx {:?}", transaction);
                transaction.set_nonce(
                    self.nonce_for_account(from_address)
                        .await
                        .map_err(|e| jrpc::error(ErrorCode::InternalError, e, id.clone(), ""))?,
                );
            }
            // The nonce was provided for us, we should increment our counter
            Some(nonce) => {
                let nonce64 = nonce.as_u64();
                // This may or may not be robust to queuing transactions
                let _ = self.nonce_counter[&from_address].compare_exchange(
                    nonce64,
                    nonce64 + 1,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                );
            }
        }

        // Defer to ethers for gas estimation, if we have an available provider
        if let Some(provider) = &self.provider {
            provider
                .fill_transaction(&mut transaction, None)
                .await
                .map_err(|e| {
                    jrpc::error(
                        ErrorCode::GasEstimationError,
                        "Unable to estimate gas prices".to_string(),
                        id.clone(),
                        e,
                    )
                })?;
        }

        tracing::debug!("Signing trx {:?}", transaction);
        let sig = wallet.sign_transaction(&transaction).await.map_err(|e| {
            jrpc::error(
                ErrorCode::InternalError,
                "Failed to sign transaction",
                id.clone(),
                e,
            )
        })?;

        Ok((sig, transaction))
    }

    async fn eth_send_transaction(&self, req: JrpcRequest) -> Result<JrpcRequest, JsonRpcErr> {
        let param_value = req.params.unwrap_or(Value::Null);
        let params: SignTransactionParams = serde_json::from_value(param_value)?;
        let (sig, transaction) = self
            .sign_transaction(req.id.clone(), params.transaction.into())
            .await?;
        let signed = transaction.rlp_signed(&sig);

        tracing::debug!(
            "Re-decoded: {:?}",
            TypedTransaction::decode_signed(&Rlp::new(&signed))
        );
        Ok(JrpcRequest::with_params(
            req.id,
            "eth_sendRawTransaction",
            to_value(vec![signed])?,
        ))
    }

    /// Responds to the `eth_signTransaction` request
    async fn eth_sign_transaction(&self, req: JrpcRequest) -> Result<Value, JsonRpcErr> {
        let param_value = req.params.unwrap_or(Value::Null);
        let params: SignTransactionParams = serde_json::from_value(param_value)?;
        let (sig, _) = self
            .sign_transaction(req.id.clone(), params.transaction.into())
            .await?;
        response(req.id, json!(sig))
    }
}

/// The params object for the `eth_signTransaction` request
/// This struct allows us to use serde rather than manually munging `Value`s
#[derive(Debug, Deserialize)]
struct SignTransactionParams {
    /// The transaction object
    transaction: TransactionRequest,
}

#[cfg(test)]
mod test {
    use std::{str::FromStr, sync::Arc};

    use super::{CredConfig, CredProxy, EthProxyConfig};
    use crate::{
        connector::passthru_pair, jrpc::no_response, transformer::eth_creds::ETH_SEND_TRANSACTION,
        JrpcRequest, JsonRpcErr, Pair,
    };

    use cubist_config::network::{
        KeystoreConfig, MnemonicConfig, PrivateKeyConfig, DEFAULT_ETH_DERIVATION_PATH_PREFIX,
    };
    use ethers::{
        core::rand::thread_rng,
        signers::{Signer, Wallet},
        types::{
            transaction::eip2718::TypedTransaction, Address, Bytes, Eip1559TransactionRequest,
            Signature, TransactionRequest,
        },
        utils::rlp::Rlp,
    };
    use futures::{FutureExt, SinkExt, StreamExt};
    use rstest::rstest;
    use serde_json::{from_value, to_value, Value};
    use tempfile::tempdir;
    pub const MNEMONIC: &str = "test test test test test test test test test test test junk";

    #[rstest]
    #[case::dp(true)]
    #[case::nodp(false)]
    fn mnemonic_accounts(#[case] use_dp: bool) {
        let proxy = CredProxy::from_cfg(&EthProxyConfig {
            onchain_uri: None,
            chain_id: 1,
            creds: vec![CredConfig::Mnemonic(MnemonicConfig {
                seed: MNEMONIC.to_string().into(),
                account_count: 2,
                derivation_path: if use_dp {
                    String::from("m/1'/2'/3'/4/")
                } else {
                    DEFAULT_ETH_DERIVATION_PATH_PREFIX.to_string()
                },
            })],
        })
        .unwrap();
        let request = JrpcRequest::new(1, "eth_accounts");
        let accounts = proxy.eth_accounts(request).unwrap();

        assert_eq!(
            accounts["result"][0],
            if use_dp {
                "0x5bd47977365798dd4b58711e2a5a20da7824618f"
            } else {
                "0x70997970c51812dc3a010c7d01b50e0d17dc79c8"
            }
        )
    }

    #[test]
    fn keystore_account() {
        let password = "hunter2".to_string();
        let name = "keyfile.json".to_string();
        let tmpdir = tempdir()
            .expect("Failed to create temporary directory")
            .into_path();
        // Create random wallet
        let (wallet, _) = Wallet::new_keystore(
            tmpdir.clone(),
            &mut thread_rng(),
            password.clone(),
            Some(&name),
        )
        .expect("Failed to create wallet");
        let proxy = CredProxy::from_cfg(&EthProxyConfig {
            onchain_uri: None,
            chain_id: 1,
            creds: vec![CredConfig::Keystore(KeystoreConfig {
                file: tmpdir.join(name),
                password: password.into(),
            })],
        })
        .expect("Failed to create proxy");
        let request = JrpcRequest::new(1, "eth_accounts");
        let accounts = proxy.eth_accounts(request).unwrap();

        assert_eq!(
            accounts["result"][0].as_str().expect("Expected string"),
            format!("0x{}", hex::encode(wallet.address().as_fixed_bytes()))
        )
    }

    /// Returns a pair of endpoints where a write to the left is sent through the [eth_creds]
    /// transformer and appears on the right side. The transformer is configured with a single
    /// account.
    fn io_pair() -> (
        impl Pair<JrpcRequest, Value, JsonRpcErr>,
        impl Pair<Value, JrpcRequest, JsonRpcErr>,
    ) {
        let config = EthProxyConfig {
            onchain_uri: None,
            chain_id: 1048576,
            creds: vec![CredConfig::PrivateKey(PrivateKeyConfig {
                hex: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
                    .to_string()
                    .into(),
            })],
        };

        // build the credential manager
        let (testin, testout) = passthru_pair::<JrpcRequest, Value, JsonRpcErr>();
        let proxy = Arc::new(CredProxy::from_cfg(&config).unwrap());
        let testout = proxy.wrap(testout);
        (Box::pin(testin), Box::pin(testout))
    }

    /// The account used in the configuration of the [eth_creds] transformer used in [io_pair()].
    const FIRST_ACCOUNT: &str = "0xc96aaa54e2d44c299564da76e1cd3184a2386b8d";

    /// Takes a transaction, performs two tests: (1) signing the transaction using the proxy and
    /// (2) sending the transaction via the proxy and have it signed during that process
    async fn test_transaction<T: Into<TypedTransaction>>(
        testin: &mut (impl Pair<JrpcRequest, Value> + Unpin),
        testout: &mut (impl Pair<Value, JrpcRequest> + Unpin),
        transaction: T,
    ) {
        let transaction: TypedTransaction = transaction.into();

        // First: Sign a transaction
        let req = JrpcRequest::with_params(
            1,
            "eth_signTransaction",
            vec![to_value(transaction.clone()).unwrap()],
        );
        testin.send(Ok(req)).await.unwrap();
        let signature: Signature =
            from_value(dbg!(testin.next().await).unwrap().unwrap()["result"].clone()).unwrap();
        assert!(testout.next().now_or_never().is_none());
        assert!(testin.next().now_or_never().is_none());

        // Then attempt to send the same transaction
        let req = JrpcRequest::with_params(
            1,
            ETH_SEND_TRANSACTION,
            vec![to_value(transaction.clone()).unwrap()],
        );
        testin.send(Ok(req)).await.unwrap();
        let send_raw = testout.next().await.unwrap().unwrap();
        assert_eq!(send_raw.method, "eth_sendRawTransaction");
        let raw_bytes: Bytes = from_value(dbg!(send_raw.params).unwrap()[0].clone()).unwrap();

        // Ensure the signature is valid and that the transaction is similar enough
        let (raw_transaction, raw_signature) =
            TypedTransaction::decode_signed(&Rlp::new(&raw_bytes)).unwrap();

        // We can't compare the full transaction because ethers overwrites None fields in our src contract
        // when it rlp encodes
        match (&transaction, &raw_transaction) {
            (
                TypedTransaction::Legacy(TransactionRequest {
                    from: in_from,
                    value: in_value,
                    ..
                }),
                TypedTransaction::Legacy(TransactionRequest {
                    from: out_from,
                    value: out_value,
                    ..
                }),
            ) => {
                // TODO: Move outside once [TypedTransaction::decode_signed()] is fixed for
                // EIP-1559 transactions
                assert_eq!(signature, raw_signature);
                assert_eq!(in_from, out_from);
                assert_eq!(in_value, out_value);
            }
            (
                TypedTransaction::Eip1559(Eip1559TransactionRequest {
                    from: _in_from,
                    value: in_value,
                    ..
                }),
                TypedTransaction::Eip1559(Eip1559TransactionRequest {
                    from: _out_from,
                    value: out_value,
                    ..
                }),
            ) => {
                // TODO: Enable once [TypedTransaction::decode_signed()] is fixed for EIP-1559
                // transactions
                // assert_eq!(in_from, out_from);
                assert_eq!(in_value, out_value);
            }
            (_, _) => panic!(
                "Transactions {:?} and {:?} are not matching",
                transaction, raw_transaction
            ),
        };
        assert!(testout.next().now_or_never().is_none());
        assert!(testin.next().now_or_never().is_none());
    }

    #[tokio::test]
    async fn test_error() {
        let (mut testin, mut testout) = io_pair();

        // errors should pass through
        let err = no_response(Some(Value::Null));
        testin.send(Err(err)).await.unwrap();
        assert!(matches!(
            testout.next().await,
            Some(Err(JsonRpcErr::Jrpc(_)))
        ));
        assert!(testout.next().now_or_never().is_none());
        assert!(testin.next().now_or_never().is_none());
    }

    #[tokio::test]
    async fn test_accounts() {
        let (mut testin, mut testout) = io_pair();

        // account list request should produce a response
        let req = JrpcRequest::new(1, "eth_accounts");
        testin.send(Ok(req)).await.unwrap();
        assert_eq!(
            testin.next().await.unwrap().unwrap()["result"][0],
            FIRST_ACCOUNT,
        );
        assert!(testout.next().now_or_never().is_none());
        assert!(testin.next().now_or_never().is_none());
    }

    #[tokio::test]
    async fn test_legacy() {
        let (mut testin, mut testout) = io_pair();

        // test legacy transaction
        let transaction = TransactionRequest::new()
            .from(Address::from_str(FIRST_ACCOUNT).unwrap())
            .value(42);
        test_transaction(&mut testin, &mut testout, transaction).await;
    }

    #[tokio::test]
    async fn test_other() {
        let (mut testin, mut testout) = io_pair();

        // other requests should pass through
        let req = JrpcRequest::new(2, "noop");
        testin.send(Ok(req)).await.unwrap();
        let resp = testout.next().await.unwrap().unwrap();
        assert_eq!((resp.method.as_str(), resp.id), ("noop", 2.into()));
        assert!(testout.next().now_or_never().is_none());
        assert!(testin.next().now_or_never().is_none());
    }
}
