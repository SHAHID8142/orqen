/**
 * Serial operation queue shared by the persistence-backed stores.
 *
 * At most one queued task runs at a time and tasks complete strictly in
 * issue order, so responses can never apply out of order. A failing task
 * does not break the queue for later tasks.
 */
export interface SerialQueue {
  run<T>(task: () => Promise<T>): Promise<T>;
}

export function createSerialQueue(): SerialQueue {
  let queue: Promise<unknown> = Promise.resolve();
  return {
    run<T>(task: () => Promise<T>): Promise<T> {
      const run = queue.then(task, task);
      queue = run.then(
        () => undefined,
        () => undefined,
      );
      return run;
    },
  };
}
