/** Whitelist metadata from existing canonical sync responses; never return wire bodies. */
export function canonicalDeletionMetadata(response) {
  let snapshot = null;
  const visit = (value, revision = null) => {
    if (!value || typeof value !== 'object') return;
    if (value.key && value.value) {
      const sequence = value.key.objId?.id?.curr;
      visit(value.value, Number.isSafeInteger(sequence) ? sequence : null);
      return;
    }
    if (Array.isArray(value.secrets) && Array.isArray(value.deletedSecrets)
      && value.deletionResults && typeof value.deletionResults === 'object' && value.vaultName) {
      const pass = (item) => {
        if (typeof item?.name !== 'string' || typeof item?.id !== 'string') throw new Error('Canonical Secret metadata has an unknown wire shape');
        return { name: item.name, id: item.id };
      };
      if (!Number.isSafeInteger(revision)) throw new Error('Canonical Vault metadata lacks its event revision');
      const observed = {
        revision,
        secrets: value.secrets.map(pass), deletedIds: value.deletedSecrets.map((item) => pass(item).id),
        states: Object.values(value.deletionResults).map((decision) => {
          const request = decision?.request;
          if (typeof request?.requestId !== 'string' || typeof decision.accepted !== 'boolean') throw new Error('Canonical deletion outcome has an unknown wire shape');
          return { secretId: pass(request.passId).id, requestId: request.requestId, clientStatus: decision.accepted ? 'Done' : 'Declined' };
        }),
      };
      if (!snapshot || observed.revision > snapshot.revision) snapshot = observed;
      return;
    }
    for (const nested of Object.values(value)) visit(nested, revision);
  };
  visit(response);
  return snapshot;
}
