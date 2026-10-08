Spike #392 (throwaway, never merged): how many engines can play at once.

  node tools/spike-392/multi.mjs     N engines on one thread, idle cost
  node tools/spike-392/workers.mjs   engines in worker_threads + a mixer loop
  node tools/spike-392/server.mjs    then open http://localhost:6392/ and in
                                     the console: await run('rings', 4, 4, 8)
                                     or await run('inline', 2, 0, 8)

Needs `make wasm` first. Results: the comment on #392.
