use std::sync::Arc;
use async_trait::async_trait;
use parking_lot::{RawRwLock, RwLock};
use parking_lot::lock_api::ArcRwLockReadGuard;
use crate::client::do_client::DigitalOceanClient;
use crate::client::do_json_protocol::DropletResponse;

#[async_trait]
pub trait DropletStore {
    async fn load_droplets(&self) -> anyhow::Result<()>;

    async fn record_droplets_metrics(&self) -> anyhow::Result<()>;

    fn list_droplets(&self) -> ArcRwLockReadGuard<RawRwLock, Vec<BasicDropletInfo>>;
}

#[derive(Clone)]
pub struct BasicDropletInfo {
    pub id: u64,
    pub name: String,
    pub memory: u64,
    pub vcpus: u64,
    pub disk: u64,
    pub locked: bool,
    pub status: String,
}

impl From<DropletResponse> for BasicDropletInfo {
    fn from(value: DropletResponse) -> Self {
        Self {
            id: value.id,
            name: value.name,
            memory: value.memory,
            vcpus: value.vcpus,
            disk: value.disk,
            locked: value.locked,
            status: value.status,
        }
    }
}


#[derive(Default)]
pub struct DropletStoreImpl<DOClient> {
    store: Arc<RwLock<Vec<BasicDropletInfo>>>,
    client: DOClient,
}


impl<DOClient: DigitalOceanClient + Clone> DropletStoreImpl<DOClient> {
    fn safe_droplets(&self,
                     droplets: Vec<BasicDropletInfo>) {
        *self.store.write() = droplets;
    }
}

#[async_trait]
impl<DOClient: DigitalOceanClient + Clone + Send + Sync> DropletStore for DropletStoreImpl<DOClient> {
    async fn load_droplets(&self) -> anyhow::Result<()> {
        let mut result: Vec<BasicDropletInfo> = Vec::new();
        let mut fetch_next = true;
        let mut page = 1u64;
        let per_page: u64 = 100u64;
        while fetch_next {
            let loaded = self.client.list_droplets(per_page, page).await?;
            fetch_next = !loaded.droplets.is_empty();
            result.extend(loaded.droplets.into_iter().map(BasicDropletInfo::from));
            page += 1;
        }
        self.safe_droplets(result);
        Ok(())
    }

    async fn record_droplets_metrics(&self) -> anyhow::Result<()> {
        todo!()
    }

    fn list_droplets(&self) -> ArcRwLockReadGuard<RawRwLock, Vec<BasicDropletInfo>> {
        self.store.read_arc()
    }
}


