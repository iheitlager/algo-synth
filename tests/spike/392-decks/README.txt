Spike #392 (throwaway, never merged): how many engines can play at once.
Needs `make wasm` first. Results: the comment on #392.

  node tests/spike/392-decks/multi.mjs     N engines on one thread, idle cost
  node tests/spike/392-decks/workers.mjs   engines in worker_threads + a mixer loop
  node tests/spike/392-decks/server.mjs    then open http://localhost:6392/ in
                                           Chrome and click a run
