use async_trait::async_trait;

#[async_trait]
pub trait DropletStore {
    async fn load_droplets(&self) -> anyhow::Result<()>;

    async fn record_droplets_metrics(&self) -> anyhow::Result<()>;

    fn list_droplets(&self) -> Vec<BasicDropletInfo>;
}

pub struct BasicDropletInfo {
    pub id: u64,
    pub name: String
}

pub struct DropletStoreImpl {

}

#[async_trait]
impl DropletStore for DropletStoreImpl {
    async fn load_droplets(&self) -> anyhow::Result<()> {
        todo!()
    }

    async fn record_droplets_metrics(&self) -> anyhow::Result<()> {
        todo!()
    }

    fn list_droplets(&self) -> Vec<BasicDropletInfo> {
        todo!()
    }
}