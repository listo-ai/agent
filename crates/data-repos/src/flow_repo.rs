use async_trait::async_trait;
use data_entities::FlowId;

use crate::RepoError;

/// Opaque flow record. Physical shape lives per-backend; this is the
/// logical projection domain code sees.
#[derive(Debug, Clone)]
pub struct Flow {
    pub id: FlowId,
    pub name: String,
    pub document: Vec<u8>, // serialized flow.schema.json payload
}

#[derive(Debug, Clone, Default)]
pub struct FlowQuery {
    pub name_contains: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[async_trait]
pub trait FlowRepo: Send + Sync + 'static {
    async fn get(&self, id: FlowId) -> Result<Flow, RepoError>;
    async fn save(&self, flow: &Flow) -> Result<(), RepoError>;
    async fn delete(&self, id: FlowId) -> Result<(), RepoError>;
    async fn list(&self, query: FlowQuery) -> Result<Vec<Flow>, RepoError>;
}
