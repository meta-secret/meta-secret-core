//! Shared non-revealing local distribution loading and stored-share validation.
use super::{
    data_block::{common::SharedSecretConfig, encrypted_data_block::SECRET_DATA_BLOCK_SIZE},
    shared_secret::UserShareDto,
};
use crate::node::{
    common::model::{
        meta_pass::MetaPasswordId,
        secret::{SecretDistributionData, SsDistributionId},
        user::user_creds::UserCreds,
    },
    db::{
        descriptors::{
            object_descriptor::ToObjectDescriptor, shared_secret_descriptor::SsWorkflowDescriptor,
        },
        events::{object_id::ArtifactId, shared_secret_event::SsWorkflowObject},
        objects::persistent_object::PersistentObject,
        repo::generic_db::KvLogEventRepo,
    },
};
use anyhow::{Result, ensure};

/// Current policy is chosen by the owning Core action, never by stored data.
pub enum LocalShareProfile {
    WholeCopy,
    Exact(SharedSecretConfig),
}

/// Callers retain their current membership/Pass eligibility guard. This loader
/// adds no canonical-claim/history dependency to existing public local Show.
pub async fn load_local_distribution<R: KvLogEventRepo>(
    p_obj: &PersistentObject<R>,
    creds: &UserCreds,
    pass: &MetaPasswordId,
) -> Result<Option<SecretDistributionData>> {
    load_local_distribution_source(p_obj, creds, pass)
        .await?
        .map(|event| event.to_distribution_data())
        .transpose()
}

/// Preserves the exact validated ciphertext object for checked result metadata.
pub async fn load_local_distribution_source<R: KvLogEventRepo>(
    p_obj: &PersistentObject<R>,
    creds: &UserCreds,
    pass: &MetaPasswordId,
) -> Result<Option<SsWorkflowObject>> {
    let desc = SsWorkflowDescriptor::Distribution(SsDistributionId {
        pass_id: pass.clone(),
        receiver: creds.device_id().clone(),
    });
    let Some(event) = p_obj.find_tail_event(desc.clone()).await? else {
        return Ok(None);
    };
    let SsWorkflowObject::Distribution(event) = event else {
        anyhow::bail!("MaterialIdentityMismatch");
    };
    ensure!(
        event.key.obj_desc == desc.clone().to_obj_desc()
            && event.key.obj_id.fqdn == ArtifactId::from(desc).fqdn
            && event.value.claim_id.pass_id == *pass
            && event.value.vault_name == creds.vault_name,
        "MaterialIdentityMismatch"
    );
    Ok(Some(SsWorkflowObject::Distribution(event)))
}

/// Decodes only the Encrypted Key Share DTO; never combines/reconstructs a Secret.
pub fn decode_local_share(
    data: &SecretDistributionData,
    creds: &UserCreds,
    profile: LocalShareProfile,
) -> Result<UserShareDto> {
    let cipher = data.secret_message.cipher_text();
    ensure!(
        cipher.channel.receiver().to_device_id() == *creds.device_id()
            && *cipher.channel.receiver() == creds.device().keys.transport_pk,
        "MaterialRecipientMismatch"
    );
    let clear = cipher.decrypt(&creds.device_creds.secret_box.transport.sk)?;
    let share = UserShareDto::try_from(&clear.msg)?;
    validate_local_share(&share, profile)?;
    Ok(share)
}

/// The same shape check for an already decrypted actual reconstruction input.
pub(crate) fn validate_local_share(share: &UserShareDto, profile: LocalShareProfile) -> Result<()> {
    let stored = share
        .share_blocks
        .first()
        .ok_or_else(|| anyhow::anyhow!("MaterialShapeMismatch"))?
        .config;
    match profile {
        LocalShareProfile::WholeCopy => ensure!(
            stored.threshold == 1 && matches!(stored.number_of_shares, 1 | 2),
            "MaterialShapeMismatch"
        ),
        LocalShareProfile::Exact(expected) => ensure!(
            stored.threshold == expected.threshold
                && stored.number_of_shares == expected.number_of_shares,
            "MaterialShapeMismatch"
        ),
    }
    ensure!(
        share.share_id > 0 && share.share_id <= stored.number_of_shares,
        "MaterialShapeMismatch"
    );
    for (index, block) in share.share_blocks.iter().enumerate() {
        let bytes = Vec::<u8>::try_from(&block.data)?;
        ensure!(
            block.block == index
                && block.config.threshold == stored.threshold
                && block.config.number_of_shares == stored.number_of_shares
                && bytes.len() == SECRET_DATA_BLOCK_SIZE
                && bytes.iter().any(|b| *b != 0)
                && block.meta_data.size > 0
                && block.meta_data.size <= SECRET_DATA_BLOCK_SIZE,
            "MaterialShapeMismatch"
        );
    }
    Ok(())
}

/// Block compatibility only; callers retain their separate share-ID checks.
pub(crate) fn compatible_share_blocks(a: &UserShareDto, b: &UserShareDto) -> bool {
    a.share_blocks.len() == b.share_blocks.len()
        && a.share_blocks.iter().zip(&b.share_blocks).all(|(a, b)| {
            a.block == b.block
                && a.meta_data == b.meta_data
                && a.config.threshold == b.config.threshold
                && a.config.number_of_shares == b.config.number_of_shares
        })
}
