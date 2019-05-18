I'm developing essentially four libraries in tandem:
1. reo_rs
2. <yourproto>.rs
3. reo_api_gen.rs
4. <yourapi>.rs

{2,3,4} depend on 1
{4} depends on 3


We have two ways to group libraries:
1. are they "inside" Reo?
2. do they need to use sensitive functions?

as unsafe{} rust allows one to do arbitrary memory operations,  