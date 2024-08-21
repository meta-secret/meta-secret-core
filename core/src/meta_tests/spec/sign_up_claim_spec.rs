use std::sync::Arc;

use crate::{
    meta_tests::spec::device_log_spec::DeviceLogSpec,
    node::{
        common::model::user::UserData,
        db::{objects::persistent_object::PersistentObject, repo::generic_db::KvLogEventRepo},
    },
};
use anyhow::Result;

use super::{ss_device_log_spec::SSDeviceLogSpec, test_spec::TestSpec};
use async_trait::async_trait;

pub struct SignUpClaimSpec<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub user: UserData,
}

#[async_trait(? Send)]
impl<Repo: KvLogEventRepo> TestSpec for SignUpClaimSpec<Repo> {
    async fn verify(&self) -> Result<()> {
        let device_log_spec = DeviceLogSpec {
            p_obj: self.p_obj.clone(),
            user: self.user.clone(),
        };

        device_log_spec.check_initialization().await?;
        device_log_spec.check_sign_up_request().await?;

        let ss_device_log_spec = SSDeviceLogSpec {
            p_obj: self.p_obj.clone(),
            client_user: self.user.clone(),
        };

        ss_device_log_spec.check_initialization().await?;

        Ok(())
    }
}
