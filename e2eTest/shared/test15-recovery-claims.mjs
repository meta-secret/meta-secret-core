export function recoveryClaimIds(claimsInfo, password) {
  return new Set((claimsInfo.claims ?? [])
    .filter((claim) => claim.type === 'Recover' && claim.password === password)
    .map((claim) => claim.id));
}

export function selectNewPendingRecoveryClaim(claimsInfo, {
  password,
  baselineClaimIds,
  receiverId,
  senderId,
}) {
  const candidates = (claimsInfo.claims ?? []).filter((claim) => (
    claim.type === 'Recover'
      && claim.password === password
      && !baselineClaimIds.has(claim.id)
      && (!senderId || claim.sender === senderId)
      && (!receiverId || (claim.receivers ?? []).some((receiver) => (
        receiver.id === receiverId && receiver.status === 'Pending'
      )))
  ));
  if (candidates.length !== 1) return null;
  return candidates[0];
}
