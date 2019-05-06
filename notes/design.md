Properties of the runtime:
1. Locking is at the per-port granularity
1. Ports are created as a _pair_ of items: (Putter, Getter) with independent lifetimes