use crate::node::db::db::{MetaVaultRepo, SaveCommand};
use crate::models::device_info::DeviceInfo;
use crate::models::meta_vault::MetaVault;
use crate::crypto;

//запилить клиентскую часть!
//нам надо регистрацию мета волта взять из веб cli
//потом юзер креденшиалы
//потом синхронизацию настроить с серваком
//и наконец - функциональность приложения

pub struct MetaVaultService<T: MetaVaultRepo> {
    pub repo: T
}

impl <T: MetaVaultRepo> MetaVaultService<T> {

    pub async fn create_meta_vault(&self, vault_name: &str, device_name: &str) -> Result<(), <T as SaveCommand<MetaVault>>::Error> {
        let device = DeviceInfo {
            device_id: crypto::utils::generate_hash(),
            device_name: device_name.to_string(),
        };

        let meta_vault = MetaVault {
            name: vault_name.to_string(),
            device: Box::new(device),
        };

        self.repo
            .save("vault", &meta_vault)
            .await
    }
}
