// Keep reusable suite outputs unless cheaper approved cleanup cannot make room.
export function reclaimBuildCaches({ measureFreeGiB, minimumGiB, buildersIdle, cleanups, report, preservePreparedBuilds = false }) {
  let freeGiB = measureFreeGiB();
  if (freeGiB >= minimumGiB) {
    report(`preserved reusable build caches: ${freeGiB} GiB meets ${minimumGiB} GiB minimum`);
    return freeGiB;
  }
  if (preservePreparedBuilds) {
    report(`preserved active prepared products: build-cache fallback blocked below ${minimumGiB} GiB; pause required`);
    return freeGiB;
  }
  if (!buildersIdle()) {
    report('preserved reusable build caches: a build is active or process state is unavailable');
    return freeGiB;
  }
  for (const cleanup of cleanups) {
    // An expensive fallback is justified only by this fresh disk measurement.
    if (freeGiB >= minimumGiB) break;
    cleanup();
    freeGiB = measureFreeGiB();
  }
  return freeGiB;
}
