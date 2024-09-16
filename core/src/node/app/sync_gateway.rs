use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, bail};
use tracing::{debug, error, info, instrument};

use crate::node::common::model::crypto::{AeadAuthData, AeadPlainText, EncryptedMessage};
use crate::node::common::model::device::{DeviceData, DeviceId, DeviceLink};
use crate::node::common::model::secret::{
    SSDistributionId, SSDistributionStatus, SecretDistributionData, SecretDistributionType,
};
use crate::node::common::model::user::{UserCredentials, UserDataMember, UserId};
use crate::node::common::model::vault::VaultStatus;
use crate::node::db::descriptors::global_index_descriptor::GlobalIndexDescriptor;
use crate::node::db::descriptors::object_descriptor::{ObjectDescriptor, ToObjectDescriptor};
use crate::node::db::descriptors::shared_secret_descriptor::SharedSecretDescriptor;
use crate::node::db::descriptors::vault_descriptor::VaultDescriptor;
use crate::node::db::events::db_tail::DbTail;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, KeyExtractor, ToGenericEvent};
use crate::node::db::events::global_index_event::GlobalIndexObject;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::local_event::{CredentialsObject, DbTailObject};
use crate::node::db::events::object_id::ObjectId;
use crate::node::db::events::shared_secret_event::SharedSecretObject;
use crate::node::db::objects::global_index::ClientPersistentGlobalIndex;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::credentials_repo::CredentialsRepo;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::server::request::{GlobalIndexRequest, SyncRequest, VaultRequest};
use crate::node::server::server_app::ServerDataTransfer;
use crate::node::server::server_data_sync::{DataEventsResponse, DataSyncRequest, ServerTailResponse};

pub struct SyncGateway<Repo: KvLogEventRepo> {
    pub id: String,
    pub persistent_object: Arc<PersistentObject<Repo>>,
    pub server_dt: Arc<ServerDataTransfer>,
}

impl<Repo: KvLogEventRepo> SyncGateway<Repo> {
    #[instrument(skip_all)]
    pub async fn run(&self) {
        info!("Run sync gateway");

        loop {
            let result = self.sync().await;
            if let Err(err) = result {
                error!("Sync error: {:?}", err);
            }

            async_std::task::sleep(Duration::from_millis(300)).await;
        }
    }

    ///Levels of synchronization:
    ///  - global index, server PK - when user has no account
    ///  - vault, shared secret... - user has been registered, we can sync vault related events
    #[instrument(skip_all)]
    pub async fn sync(&self) -> anyhow::Result<()> {
        let creds_repo = CredentialsRepo {
            p_obj: self.persistent_object.clone(),
        };

        let maybe_creds_event = creds_repo.find().await?;

        let Some(creds_obj) = maybe_creds_event else {
            return Ok(());
        };

        self.sync_global_index(creds_obj.device()).await?;

        let CredentialsObject::DefaultUser(user_creds_event) = creds_obj else {
            return Ok(());
        };

        //Vault synchronization
        let user_creds = user_creds_event.value;
        let sender = user_creds.user();

        let p_vault = PersistentVault {
            p_obj: self.persistent_object.clone(),
        };
        let vault_status = p_vault.find_for_default_user().await?;

        let Some(VaultStatus::Member { member, .. }) = vault_status else {
            return Ok(());
        };

        let vault_sync_request = {
            let vault_name = user_creds.vault_name.clone();

            let vault_log_free_id = {
                let obj_desc = VaultDescriptor::vault_log(vault_name.clone());
                self.persistent_object.find_free_id_by_obj_desc(obj_desc).await?
            };

            let vault_free_id = {
                let obj_desc = VaultDescriptor::vault(vault_name.clone());
                self.persistent_object.find_free_id_by_obj_desc(obj_desc).await?
            };

            let vault_status_free_id = {
                let obj_desc = VaultDescriptor::vault_membership(user_creds.user_id());
                self.persistent_object.find_free_id_by_obj_desc(obj_desc).await?
            };

            SyncRequest::Vault(VaultRequest {
                sender,
                vault_log: vault_log_free_id,
                vault: vault_free_id,
                vault_status: vault_status_free_id,
            })
        };

        let DataEventsResponse(data_sync_events) = self
            .server_dt
            .dt
            .send_to_service_and_get(DataSyncRequest::SyncRequest(vault_sync_request))
            .await?
            .to_data()?;

        for new_event in data_sync_events {
            debug!("id: {:?}. Sync gateway. New event: {:?}", self.id, new_event);
            self.persistent_object.repo.save(new_event).await?;
        }

        // TODO Get latest device log messages and send to the server
        //  - get the latest device_log and ss_device_log tail ids from server
        let server_tail_response = self
            .server_dt
            .dt
            .send_to_service_and_get(DataSyncRequest::ServerTailRequest(member.user().user_id()))
            .await?
            .to_server_tail()?;

        self.sync_device_log(&server_tail_response, user_creds.user_id())
            .await?;
        self.sync_shared_secrets(&server_tail_response, &user_creds).await?;

        Ok(())
    }

    async fn sync_ss_device_log(&self, server_tail: &ServerTailResponse, device_id: DeviceId) -> anyhow::Result<()> {
        let server_ss_device_log_tail_id = server_tail
            .ss_device_log_tail
            .clone()
            .unwrap_or_else(|| ObjectId::unit(SharedSecretDescriptor::SSDeviceLog(device_id).to_obj_desc()));

        let ss_device_log_events_to_sync = self
            .persistent_object
            .find_object_events(server_ss_device_log_tail_id)
            .await?;

        for event in ss_device_log_events_to_sync {
            self.server_dt.dt.send_to_service(DataSyncRequest::Event(event)).await;
        }

        Ok(())
    }

    async fn sync_device_log(&self, server_tail: &ServerTailResponse, user_id: UserId) -> anyhow::Result<()> {
        let server_device_log_tail_id = server_tail
            .device_log_tail
            .clone()
            .unwrap_or_else(|| ObjectId::unit(VaultDescriptor::device_log(user_id)));

        let device_log_events_to_sync = self
            .persistent_object
            .find_object_events(server_device_log_tail_id)
            .await?;

        for event in device_log_events_to_sync {
            self.server_dt.dt.send_to_service(DataSyncRequest::Event(event)).await;
        }

        Ok(())
    }

    async fn sync_global_index(&self, sender: DeviceData) -> anyhow::Result<()> {
        //TODO optimization: read global index tail id from db_tail

        let gi_free_id = {
            let gi_desc = ObjectDescriptor::GlobalIndex(GlobalIndexDescriptor::Index);
            self.persistent_object.find_free_id_by_obj_desc(gi_desc).await?
        };

        let sync_request = SyncRequest::GlobalIndex(GlobalIndexRequest {
            sender,
            global_index: gi_free_id,
        });

        let DataEventsResponse(new_gi_events) = self
            .server_dt
            .dt
            .send_to_service_and_get(DataSyncRequest::SyncRequest(sync_request))
            .await?
            .to_data()?;

        let p_gi_obj = ClientPersistentGlobalIndex { p_obj: self.persistent_object.clone()};
        for gi_event in new_gi_events {
            if let GenericKvLogEvent::GlobalIndex(gi_obj) = &gi_event {
                p_gi_obj.save(gi_obj).await?;
            } else {
                return Err(anyhow!("Invalid event: {:?}", gi_event.key().obj_desc()));
            }
        }
        Ok(())
    }

    #[instrument(skip_all)]
    async fn sync_shared_secrets(
        &self,
        server_tail: &ServerTailResponse,
        creds: &UserCredentials,
    ) -> anyhow::Result<()> {
        self.sync_ss_device_log(&server_tail, creds.device().id).await?;

        let key_manager = creds.device_creds.key_manager()?;

        let p_vault = PersistentVault {
            p_obj: self.persistent_object.clone(),
        };

        let vault_status = p_vault.find(creds.user()).await?;

        match vault_status {
            VaultStatus::Outsider(_) => {
                Ok(())
            }
            VaultStatus::Member {
                vault,
                member: UserDataMember(member_data),
            } => {
                //get ss_ledger
                // distribute shares if needed
                let persistent_ss = PersistentSharedSecret {
                    p_obj: self.persistent_object.clone(),
                };

                let ledger_obj = persistent_ss.get_ledger(member_data.vault_name).await?;
                let ledger_data = ledger_obj.to_ledger_data()?;

                for (claim_id, claim) in ledger_data.claims {
                    for (p2p_device_link, status) in claim.distribution {
                        if !p2p_device_link.receiver().eq(&member_data.device.id) {
                            continue;
                        }

                        match claim.distribution_type {
                            SecretDistributionType::Recover => {
                                //send share
                                if let SSDistributionStatus::Pending = status {
                                    let local_share = persistent_ss
                                        .get_local_share_distribution_data(claim.pass_id.clone())
                                        .await?;

                                    // TODO decrypt local share message

                                    let plain_share = {
                                        let encrypted_local_share = local_share.secret_message.cipher_text();
                                        encrypted_local_share.decrypt(&key_manager.transport.secret_key)?
                                    };

                                    let device_link = DeviceLink::PeerToPeer(p2p_device_link.clone());
                                    let maybe_channel = vault.build_communication_channel(device_link);

                                    let Some(channel) = maybe_channel else {
                                        bail!("Failed to build communication channel")
                                    };

                                    // Since we got a device link from a claim it means the sender of a claim
                                    // going to be the receiver of the share.
                                    // We need to swap the sender and the receiver
                                    let inverse_channel = channel.inverse();

                                    //get user from a vault
                                    let plain_text_response = {
                                        AeadPlainText {
                                            msg: plain_share.msg,
                                            auth_data: AeadAuthData::from(inverse_channel),
                                        }
                                    };

                                    let inverse_p2p_link = p2p_device_link.inverse();
                                    let inverse_device_link = DeviceLink::PeerToPeer(inverse_p2p_link.clone());

                                    let ss_dist = {
                                        let sk = &key_manager.transport.secret_key;
                                        let encrypted_share = plain_text_response.encrypt(sk)?;
                                        let encrypted_message = EncryptedMessage::CipherShare {
                                            device_link: inverse_device_link.clone(),
                                            share: encrypted_share,
                                        };

                                        SecretDistributionData {
                                            vault_name: vault.vault_name.clone(),
                                            pass_id: claim.pass_id.clone(),
                                            secret_message: encrypted_message,
                                        }
                                    };

                                    let ss_recover_obj = {
                                        let ss_dist_obj_desc = {
                                            let ss_event_id = SSDistributionId {
                                                claim_id: claim_id.clone(),
                                                distribution_type: SecretDistributionType::Recover,
                                                device_link: inverse_p2p_link,
                                            };

                                            SharedSecretDescriptor::SSDistribution(ss_event_id).to_obj_desc()
                                        };

                                        let key = KvLogEvent {
                                            key: KvKey::unit(ss_dist_obj_desc),
                                            value: ss_dist,
                                        };

                                        SharedSecretObject::SSDistribution(key).to_generic()
                                    };

                                    self.server_dt
                                        .dt
                                        .send_to_service(DataSyncRequest::Event(ss_recover_obj))
                                        .await;
                                }
                            }
                            SecretDistributionType::Split => {
                                // NOOP
                            }
                        }
                    }
                }

                Ok(())
            }
        }
    }

    #[instrument(skip_all)]
    async fn save_updated_db_tail(&self, db_tail: DbTail, new_db_tail: DbTail) -> anyhow::Result<()> {
        if new_db_tail == db_tail {
            return Ok(());
        }

        //update db_tail
        let new_db_tail_event = DbTailObject(KvLogEvent {
            key: KvKey::unit(ObjectDescriptor::DbTail),
            value: new_db_tail.clone(),
        })
        .to_generic();

        self.persistent_object.repo.save(new_db_tail_event).await?;
        Ok(())
    }
}
