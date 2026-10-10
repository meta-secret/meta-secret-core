// Invoke only after the cycle's reveal/close markers and actor results settle.
// Each declared boundary action runs once, before the next cycle may start.
export function createPostCycleActions(actions) {
  const completed = new Set();
  for (const action of actions) {
    if (!Number.isInteger(action.afterCycle) || action.afterCycle < 1) {
      throw new Error(`Invalid post-cycle boundary for ${action.name}`);
    }
  }
  return async function afterCompletedCycle(cycleNumber) {
    if (completed.has(cycleNumber)) throw new Error(`Cycle ${cycleNumber} completion was already handled`);
    for (const action of actions) {
      if (action.afterCycle === cycleNumber) await action.run(cycleNumber);
    }
    completed.add(cycleNumber);
  };
}
