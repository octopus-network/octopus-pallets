//! RPC interface for the transaction payment module.

use jsonrpc_core::{Error as RpcError, ErrorCode, Result};
use jsonrpc_derive::rpc;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::{generic::BlockId, traits::Block as BlockT};
use std::sync::Arc;
use pallet_ibc_runtime_api::IbcApi as IbcRuntimeApi;

#[rpc]
pub trait IbcApi<BlockHash> {
    #[rpc(name = "get_identified_any_client_state")]
    fn get_identified_any_client_state(&self, at: Option<BlockHash>) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;

    #[rpc(name = "get_idenfitied_connection_end")]
    fn get_idenfitied_connection_end(&self, at: Option<BlockHash>) -> Result<Vec<(Vec<u8>, Vec<u8>)>>;

    #[rpc(name = "get_idenfitied_channel_end")]
    fn get_idenfitied_channel_end(&self, at: Option<BlockHash>) -> Result<Vec<(Vec<u8>, Vec<u8>, Vec<u8>)>>;

    #[rpc(name = "get_channel_end")]
    fn get_channel_end(&self, port_id: Vec<u8>, channel_id: Vec<u8>, at: Option<BlockHash>) -> Result<Vec<u8>>;

    #[rpc(name = "get_packet_commitment_state")]
    fn get_packet_commitment_state(&self, at: Option<BlockHash>) -> Result<Vec<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>)>>;

    #[rpc(name = "get_packet_acknowledge_state")]
    fn get_packet_acknowledge_state(&self, at: Option<BlockHash>) -> Result<Vec<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>)>>;
}

/// A struct that implements the `ConsensusStateWithHeightApi`.
pub struct IbcStorage<C, M> {
    // If you have more generics, no need to SumStorage<C, M, N, P, ...>
    // just use a tuple like SumStorage<C, (M, N, P, ...)>
    client: Arc<C>,
    _marker: std::marker::PhantomData<M>,
}

impl<C, M> IbcStorage<C, M> {
    /// Create new `ConsensusStateWithHeightStorage` instance with the given reference to the client.
    pub fn new(client: Arc<C>) -> Self {
        Self {
            client,
            _marker: Default::default(),
        }
    }
}


impl<C, Block> IbcApi<<Block as BlockT>::Hash> for IbcStorage<C, Block>
    where
        Block: BlockT,
        C: Send + Sync + 'static,
        C: ProvideRuntimeApi<Block>,
        C: HeaderBackend<Block>,
        C::Api: IbcRuntimeApi<Block>,
{
    fn get_identified_any_client_state(&self, at: Option<<Block as BlockT>::Hash>)
        -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let api = self.client.runtime_api();
        let at = BlockId::hash(at.unwrap_or_else(||
            // If the block hash is not supplied assume the best block.
            self.client.info().best_hash));

        let runtime_api_result = api.get_identified_any_client_state(&at);
        runtime_api_result.map_err(|e| RpcError {
            code: ErrorCode::ServerError(9876), // No real reason for this value
            message: "Something wrong".into(),
            data: Some(format!("{:?}", e).into()),
        })
    }

    fn get_idenfitied_connection_end(&self, at: Option<<Block as BlockT>::Hash>)
        -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let api = self.client.runtime_api();
        let at = BlockId::hash(at.unwrap_or_else(||
            // If the block hash is not supplied assume the best block.
            self.client.info().best_hash));

        let runtime_api_result = api.get_idenfitied_connection_end(&at);
        runtime_api_result.map_err(|e| RpcError {
            code: ErrorCode::ServerError(9876), // No real reason for this value
            message: "Something wrong".into(),
            data: Some(format!("{:?}", e).into()),
        })
    }

    fn get_idenfitied_channel_end(&self, at: Option<<Block as BlockT>::Hash>)
        -> Result<Vec<(Vec<u8>, Vec<u8>, Vec<u8>)>> {
        let api = self.client.runtime_api();
        let at = BlockId::hash(at.unwrap_or_else(||
            // If the block hash is not supplied assume the best block.
            self.client.info().best_hash));

        let runtime_api_result = api.get_idenfitied_channel_end(&at);
        runtime_api_result.map_err(|e| RpcError {
            code: ErrorCode::ServerError(9876), // No real reason for this value
            message: "Something wrong".into(),
            data: Some(format!("{:?}", e).into()),
        })
    }

    fn get_channel_end(&self, port_id: Vec<u8>, channel_id: Vec<u8>, at: Option<<Block as BlockT>::Hash>) -> Result<Vec<u8>> {
        let api = self.client.runtime_api();
        let at = BlockId::hash(at.unwrap_or_else(||
            // If the block hash is not supplied assume the best block.
            self.client.info().best_hash));

        let runtime_api_result = api.get_channel_end(&at, port_id, channel_id);
        runtime_api_result.map_err(|e| RpcError {
            code: ErrorCode::ServerError(9876), // No real reason for this value
            message: "Something wrong".into(),
            data: Some(format!("{:?}", e).into()),
        })
    }

    fn get_packet_commitment_state(&self, at: Option<<Block as BlockT>::Hash>)
        -> Result<Vec<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>)>> {
        let api = self.client.runtime_api();
        let at = BlockId::hash(at.unwrap_or_else(||
            // If the block hash is not supplied assume the best block.
            self.client.info().best_hash));

        let runtime_api_result = api.get_packet_commitment_state(&at);
        runtime_api_result.map_err(|e| RpcError {
            code: ErrorCode::ServerError(9876), // No real reason for this value
            message: "Something wrong".into(),
            data: Some(format!("{:?}", e).into()),
        })
    }

    fn get_packet_acknowledge_state(&self, at: Option<<Block as BlockT>::Hash>)
                                   -> Result<Vec<(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>)>> {
        let api = self.client.runtime_api();
        let at = BlockId::hash(at.unwrap_or_else(||
            // If the block hash is not supplied assume the best block.
            self.client.info().best_hash));

        let runtime_api_result = api.get_packet_acknowledge_state(&at);
        runtime_api_result.map_err(|e| RpcError {
            code: ErrorCode::ServerError(9876), // No real reason for this value
            message: "Something wrong".into(),
            data: Some(format!("{:?}", e).into()),
        })
    }
}
