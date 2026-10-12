export function deletionCycleProgress(block, cycle) {
  const roles = block.members.map((platform) => {
    const role = platform === cycle.requester ? 'requester' : platform === cycle.offline ? 'offline'
      : platform === cycle.responder ? cycle.outcome === 'accepted' ? 'approver' : 'decliner' : 'observer';
    return `${platform}:${role}`;
  });
  return `📱 Test50 block=${block.id} cycle=${cycle.number} roles=${roles.join(', ')} outcome=${cycle.outcome}`;
}
