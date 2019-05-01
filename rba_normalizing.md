given some RBA, we want to create Norm(RBA) such that
it has NO "silent" rules, and a full enumeration
has NO FEWER possible ports reachable on all paths.

Bonus: No more paths.

eg: start: {-a,-b,-c} with memvars {a,b,c} atomic port set {1,2}
1. {a,b} =={1,3}==> {}
2. {-c} =={3}==> {}