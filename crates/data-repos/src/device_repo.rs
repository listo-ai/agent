use async_trait::async_trait;
use data_entities::DeviceId;

use crate::RepoError;

#[derive(Debug, Clone)]
pub struct Device {
    pub id: DeviceId,
    pub kind_id: String, // reverse-DNS, e.g. "acme.driver.bacnet.device"
    pub name: String,
}

#[derive(Debug, Clone, Default)]
pub struct DeviceQuery {
    pub kind_id: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[async_trait]
pub trait DeviceRepo: Send + Sync + 'static {
    async fn get(&self, id: DeviceId) -> Result<Device, RepoError>;
    async fn save(&self, device: &Device) -> Result<(), RepoError>;
    async fn delete(&self, id: DeviceId) -> Result<(), RepoError>;
    async fn list(&self, query: DeviceQuery) -> Result<Vec<Device>, RepoError>;
}
